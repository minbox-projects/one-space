import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertCircle,
  ArchiveRestore,
  Check,
  ChevronDown,
  ChevronUp,
  Clock,
  Eye,
  EyeOff,
  Info,
  Plus,
  RotateCcw,
  Server,
  Sparkles,
  Tag,
  Trash2,
  X,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import {
  draftToPriceRow,
  formatGatewayTimestamp,
  isMappingDeprecated,
  mappedUpstreamModels,
  normalizeReasoningEfforts,
  priceRowToDraft,
  resolveProviderPriceRow,
  type GatewayModelMapping,
  type GatewayPriceDraft,
  type GatewayProviderTemplateView,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
  type ModelPrice,
} from "@/lib/aiGateway";
import { MappingPriceEditor } from "./MappingPriceEditor";
import {
  PROVIDER_CUSTOM_ICON_OPTIONS,
  ProviderTemplateAvatar,
  ProviderTemplateIconPicker,
  resolveEffectiveProviderIcon,
} from "./ProviderTemplateIcon";

type ProviderDetailDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  provider: GatewayUpstreamProvider | null;
  /**
   * Latest persisted snapshot of this provider. Only the five runtime health
   * fields of rows matching the trimmed `(local_model, upstream_model)` key are
   * merged into the local draft; user-editable fields and unsaved edits stay intact.
   */
  runtimeProvider?: GatewayUpstreamProvider | null;
  prices?: ModelPrice[];
  busy: boolean;
  onSave: (provider: GatewayUpstreamProvider, prices: ModelPrice[]) => void;
  onDelete?: (providerId: string) => Promise<boolean | void> | void;
  templates?: GatewayProviderTemplateView[];
  availableTags?: string[];
  onDeleteModel?: (providerId: string, upstreamModel: string) => void;
  onRestoreModel?: (providerId: string, upstreamModel: string) => void;
  /** Clear one auto-disabled mapping row's runtime state (no `enabled` change). */
  onReenableModel?: (
    providerId: string,
    localModel: string,
    upstreamModel: string,
  ) => void;
  /** Clear every auto-disabled row of this provider (no `enabled` change). */
  onReenableModels?: (providerId: string) => void;
};

const mappingInputClass =
  "h-[38px] rounded-lg border border-border bg-background px-3 text-sm text-foreground outline-none transition-all focus:border-primary focus:ring-2 focus:ring-primary/50";

/** Stable identity for a mapping row's trimmed `(local_model, upstream_model)` key. */
function runtimeMappingKey(localModel: string, upstreamModel: string): string {
  return JSON.stringify([localModel.trim(), upstreamModel.trim()]);
}

export function ProviderDetailDialog({
  open,
  onOpenChange,
  provider,
  runtimeProvider,
  prices,
  busy,
  onSave,
  onDelete,
  templates,
  availableTags,
  onDeleteModel,
  onRestoreModel,
  onReenableModel,
  onReenableModels,
}: ProviderDetailDialogProps) {
  const { t } = useTranslation();

  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [defaultModel, setDefaultModel] = useState("");
  const [protocol, setProtocol] = useState<GatewayUpstreamProtocol>("chat_completions");
  const [mappings, setMappings] = useState<GatewayModelMapping[]>([]);
  const [priceDrafts, setPriceDrafts] = useState<GatewayPriceDraft[]>([]);
  const [autoAddedModel, setAutoAddedModel] = useState<string | null>(null);
  const [revealApiKey, setRevealApiKey] = useState(false);
  const [expandedMappings, setExpandedMappings] = useState<Record<number, boolean>>(
    {},
  );
  const [effortInputs, setEffortInputs] = useState<Record<number, string>>({});
  const [weight, setWeight] = useState<number | string>(provider?.weight ?? 1);
  const [icon, setIcon] = useState<string>("");
  const [tags, setTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState<string>("");
  const [confirmDeleting, setConfirmDeleting] = useState(false);

  useEffect(() => {
    setConfirmDeleting(false);
    if (!provider) {
      setName("");
      setBaseUrl("");
      setApiKey("");
      setDefaultModel("");
      setProtocol("chat_completions");
      setMappings([]);
      setPriceDrafts([]);
      setAutoAddedModel(null);
      setRevealApiKey(false);
      setExpandedMappings({});
      setEffortInputs({});
      setWeight(1);
      setIcon("");
      setTags([]);
      setTagInput("");
      return;
    }
    const providerPrices = prices ?? [];
    const baseMappings = provider.mappings ?? [];
    const trimmedDefault = (provider.default_model ?? "").trim();
    let nextMappings = baseMappings;
    let nextAutoAdded: string | null = null;
    if (
      trimmedDefault !== "" &&
      !baseMappings.some(
        (mapping) => mapping.upstream_model.trim() === trimmedDefault,
      )
    ) {
      nextMappings = [
        ...baseMappings,
        {
          local_model: trimmedDefault,
          upstream_model: trimmedDefault,
          enabled: true,
        },
      ];
      nextAutoAdded = trimmedDefault;
    }

    const seeded: GatewayPriceDraft[] = [];
    const seenModels = new Set<string>();
    for (const mapping of nextMappings) {
      const model = mapping.upstream_model.trim();
      if (model === "" || seenModels.has(model)) continue;
      seenModels.add(model);
      seeded.push(
        priceRowToDraft(
          resolveProviderPriceRow(providerPrices, provider.id, model),
          model,
          `price-${model}`,
        ),
      );
    }

    setName(provider.name);
    setBaseUrl(provider.base_url);
    setApiKey(provider.api_key);
    setDefaultModel(provider.default_model ?? "");
    setProtocol(provider.protocol ?? "chat_completions");
    setMappings(nextMappings);
    setPriceDrafts(seeded);
    setAutoAddedModel(nextAutoAdded);
    setRevealApiKey(false);
    setExpandedMappings({});
    setEffortInputs({});
    setWeight(provider.weight ?? 1);
    setIcon(provider.icon ?? "");
    setTags(provider.tags ?? []);
    setTagInput("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [provider, open]);

  const candidateTags = [
    ...new Set([...(availableTags ?? []), "LLM", "Embedding", "Vision", "Code", "Agent", "Reasoning"]),
  ].filter(Boolean);

  const handleAddTag = (rawTag: string) => {
    const trimmed = rawTag.trim();
    if (!trimmed) return;
    if (!tags.includes(trimmed)) {
      setTags((prev) => [...prev, trimmed]);
    }
    setTagInput("");
  };

  const handleRemoveTag = (tagToRemove: string) => {
    setTags((prev) => prev.filter((t) => t !== tagToRemove));
  };

  const handleToggleTag = (tag: string) => {
    if (tags.includes(tag)) {
      handleRemoveTag(tag);
    } else {
      handleAddTag(tag);
    }
  };

  const handleTagInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleAddTag(tagInput);
    } else if (e.key === "Backspace" && !tagInput && tags.length > 0) {
      handleRemoveTag(tags[tags.length - 1]);
    }
  };

  // 实时运行时合并：在弹窗打开时，把 `runtimeProvider` 快照中匹配行的五个运行时
  // 字段并入本地 mappings 草稿，绝不重置草稿或覆盖用户可编辑字段。effect 幂等：
  // 逐字段比较，无变化时返回原数组（不触发重渲染），因此可在每次渲染后安全运行
  // 而不循环，也能捕获运行时快照被原地更新（引用不变）的情况。
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!open || !runtimeProvider) return;
    const runtimeByKey = new Map<string, GatewayModelMapping>();
    for (const mapping of runtimeProvider.mappings ?? []) {
      runtimeByKey.set(
        runtimeMappingKey(mapping.local_model, mapping.upstream_model),
        mapping,
      );
    }
    setMappings((prev) => {
      let changed = false;
      const next = prev.map((mapping) => {
        const runtime = runtimeByKey.get(
          runtimeMappingKey(mapping.local_model, mapping.upstream_model),
        );
        if (!runtime) return mapping;
        if (
          mapping.auto_disabled === runtime.auto_disabled &&
          mapping.disabled_reason === runtime.disabled_reason &&
          mapping.disabled_at === runtime.disabled_at &&
          mapping.consecutive_failures === runtime.consecutive_failures &&
          mapping.last_error_at === runtime.last_error_at
        ) {
          return mapping;
        }
        changed = true;
        return {
          ...mapping,
          auto_disabled: runtime.auto_disabled,
          disabled_reason: runtime.disabled_reason,
          disabled_at: runtime.disabled_at,
          consecutive_failures: runtime.consecutive_failures,
          last_error_at: runtime.last_error_at,
        };
      });
      return changed ? next : prev;
    });
  });

  if (!provider) return null;

  const isEditing = Boolean(provider.id);
  const isTemplateBound = Boolean(provider.template_id);
  const boundTemplateView = provider.template_id
    ? templates?.find((view) => view.template.id === provider.template_id)
    : undefined;
  const boundTemplate = boundTemplateView?.template;
  const lastSyncText = boundTemplateView?.synced_at
    ? formatGatewayTimestamp(boundTemplateView.synced_at)
    : t("aiGatewayTemplateNotSynced", "Not synced yet");
  const ignoredModels = isTemplateBound ? provider.ignored_models ?? [] : [];
  // 从本地草稿派生自动禁用计数与批量重新启用控件，使合并后的运行时更新立即反映。
  const autoDisabledModels = mappings.filter(
    (mapping) => mapping.auto_disabled === true,
  );

  const draftForModel = (model: string): GatewayPriceDraft =>
    priceDrafts.find((draft) => draft.upstream_model === model) ??
    priceRowToDraft(undefined, model, `price-${model}`);

  const updateDraftForModel = (
    model: string,
    patch: Partial<GatewayPriceDraft>,
  ) => {
    setPriceDrafts((prev) => {
      if (!prev.some((draft) => draft.upstream_model === model)) {
        return [
          ...prev,
          { ...priceRowToDraft(undefined, model, `price-${model}`), ...patch },
        ];
      }
      return prev.map((draft) =>
        draft.upstream_model === model ? { ...draft, ...patch } : draft,
      );
    });
  };

  const updateMapping = (index: number, patch: Partial<GatewayModelMapping>) => {
    setMappings((prev) =>
      prev.map((entry, entryIndex) =>
        entryIndex === index ? { ...entry, ...patch } : entry,
      ),
    );
  };

  const addEffort = (index: number) => {
    const raw = effortInputs[index] ?? "";
    setMappings((prev) =>
      prev.map((entry, entryIndex) =>
        entryIndex === index
          ? {
              ...entry,
              reasoning_efforts: normalizeReasoningEfforts([
                ...(entry.reasoning_efforts ?? []),
                raw,
              ]),
            }
          : entry,
      ),
    );
    setEffortInputs((prev) => ({ ...prev, [index]: "" }));
  };

  const removeEffort = (index: number, effort: string) => {
    setMappings((prev) =>
      prev.map((entry, entryIndex) =>
        entryIndex === index
          ? {
              ...entry,
              reasoning_efforts: normalizeReasoningEfforts(
                (entry.reasoning_efforts ?? []).filter((item) => item !== effort),
              ),
            }
          : entry,
      ),
    );
  };

  const handleRemoveMapping = (index: number) => {
    const mapping = mappings[index];
    if (
      isTemplateBound &&
      onDeleteModel &&
      mapping?.upstream_model.trim()
    ) {
      onDeleteModel(provider.id, mapping.upstream_model);
    }
    const removedModel = mapping?.upstream_model.trim() ?? "";
    const nextMappings = mappings.filter((_, entryIndex) => entryIndex !== index);
    setMappings(nextMappings);
    if (
      removedModel !== "" &&
      removedModel === defaultModel.trim() &&
      !nextMappings.some((entry) => entry.upstream_model.trim() === removedModel)
    ) {
      setDefaultModel("");
    }
    if (autoAddedModel !== null && removedModel === autoAddedModel) {
      setAutoAddedModel(null);
    }
  };

  const handleSave = () => {
    const savedMappings = mappings.map((mapping) => {
      const reasoningEfforts = normalizeReasoningEfforts(mapping.reasoning_efforts);
      return {
        ...mapping,
        enabled: mapping.enabled !== false,
        display_name: mapping.display_name?.trim()
          ? mapping.display_name.trim()
          : undefined,
        protocol: mapping.protocol ? mapping.protocol : undefined,
        reasoning_efforts: reasoningEfforts.length > 0 ? reasoningEfforts : undefined,
      };
    });
    const savedModels = new Set(
      savedMappings
        .map((mapping) => mapping.upstream_model.trim())
        .filter((model) => model !== ""),
    );
    const submittedPrices: ModelPrice[] = [];
    for (const draft of priceDrafts) {
      if (!savedModels.has(draft.upstream_model.trim())) continue;
      const row = draftToPriceRow(draft);
      if (!row) continue;
      submittedPrices.push(provider.id ? { ...row, provider_id: provider.id } : row);
    }
    onSave(
      {
        ...provider,
        name: name.trim(),
        base_url: baseUrl.trim(),
        api_key: apiKey,
        default_model: defaultModel.trim() ? defaultModel.trim() : null,
        protocol,
        mappings: savedMappings,
        weight:
          Number.isInteger(Number(weight)) &&
          Number(weight) >= 1 &&
          Number(weight) <= 100
            ? Number(weight)
            : 1,
        icon: icon.trim() ? icon.trim() : null,
        tags: Array.from(new Set(tags.map((t) => t.trim()).filter(Boolean))),
      },
      submittedPrices,
    );
    onOpenChange(false);
  };

  const handleDelete = () => {
    if (!provider.id || !onDelete) return;
    setConfirmDeleting(true);
  };

  const handleCancelDelete = () => {
    setConfirmDeleting(false);
  };

  const handleConfirmDelete = async () => {
    if (!provider.id || !onDelete) return;
    setConfirmDeleting(false);
    const result = await onDelete(provider.id);
    if (result !== false) {
      onOpenChange(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="relative max-h-[90vh] w-full sm:max-w-6xl overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0"
        data-testid="ai-gateway-provider-detail"
      >
        <DialogHeader className="pl-6 pr-14 py-4 border-b bg-card/80 backdrop-blur-sm shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            {(() => {
              const effectiveIcon = resolveEffectiveProviderIcon(
                { icon, template_id: provider?.template_id, base_url: baseUrl, name, id: provider?.id },
                boundTemplate,
              );
              if (effectiveIcon) {
                return (
                  <ProviderTemplateAvatar
                    icon={effectiveIcon}
                    templateId={boundTemplate?.id ?? provider?.template_id}
                    templateName={name || boundTemplate?.name}
                    baseUrl={baseUrl}
                    size={36}
                    className="shrink-0"
                  />
                );
              }
              return (
                <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-2xs">
                  <Server className="h-4.5 w-4.5" />
                </div>
              );
            })()}
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <DialogTitle className="truncate text-base font-semibold leading-5 text-foreground">
                  {isEditing
                    ? t("aiGatewayEditProvider", "Edit provider")
                    : t("aiGatewayNewProvider", "New provider")}
                </DialogTitle>
                {isTemplateBound ? (
                  <span
                    data-testid="ai-gateway-bound-template-header-badge"
                    className="inline-flex items-center gap-1 rounded-full bg-emerald-500/10 px-2.5 py-0.5 text-[11px] font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20"
                  >
                    <Sparkles className="h-3 w-3" />
                    <span>
                      {boundTemplate
                        ? t("aiGatewayBoundTemplateBadge", {
                            name: boundTemplate.name,
                            defaultValue: `Template: ${boundTemplate.name}`,
                          })
                        : t("aiGatewayBoundTemplateNotFound", {
                            id: provider.template_id,
                            defaultValue: `Template definition not found (${provider.template_id})`,
                          })}
                    </span>
                  </span>
                ) : null}
              </div>
              <DialogDescription className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                {t(
                  "aiGatewayProviderDialogDesc",
                  "Configure upstream provider credentials, endpoint protocol, and model routing mappings.",
                )}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto p-6 space-y-4">
          {/* 关联服务商模板紧凑展示栏（当服务商关联了服务商模板时展示，低高度且完整显示 API 地址与同步时间） */}
          {isTemplateBound ? (
            <div
              data-testid="ai-gateway-bound-template-banner"
              className="rounded-xl border border-border/70 bg-muted/20 px-3.5 py-2.5 shadow-2xs space-y-1.5 transition hover:border-border"
            >
              {/* 第 1 行：品牌图标 + 模板名称 + 协议 + 预设模型数 + 最近同步时间 */}
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2 min-w-0">
                  <ProviderTemplateAvatar
                    icon={boundTemplate?.icon}
                    templateId={boundTemplate?.id ?? provider.template_id}
                    templateName={boundTemplate?.name}
                    size={26}
                    className="shrink-0"
                  />
                  <span className="font-semibold text-xs sm:text-sm text-foreground truncate">
                    {boundTemplate?.name ??
                      t("aiGatewayBoundTemplateNotFound", {
                        id: provider.template_id,
                        defaultValue: `Template definition not found (${provider.template_id})`,
                      })}
                  </span>
                  {boundTemplate ? (
                    <span
                      className={`inline-flex items-center rounded-md px-1.5 py-0.2 text-[10px] font-medium ${
                        boundTemplate.protocol === "responses"
                          ? "bg-purple-500/10 text-purple-600 dark:text-purple-400"
                          : "bg-blue-500/10 text-blue-600 dark:text-blue-400"
                      }`}
                    >
                      {boundTemplate.protocol === "responses" ? "Responses" : "Chat"}
                    </span>
                  ) : null}
                  {boundTemplate ? (
                    <span className="rounded-full bg-muted/80 px-2 py-0.2 text-[10px] font-medium text-muted-foreground border border-border/50">
                      {t("aiGatewayBoundTemplatePresetModels", {
                        count: boundTemplate.models?.length ?? 0,
                        defaultValue: `${boundTemplate.models?.length ?? 0} preset models`,
                      })}
                    </span>
                  ) : null}
                </div>

                {boundTemplate ? (
                  <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground shrink-0">
                    <Clock className="h-3 w-3 opacity-60" />
                    <span>{t("aiGatewayTemplateLastSync", "Last sync")}:</span>
                    <span className="font-medium text-foreground">{lastSyncText}</span>
                  </div>
                ) : null}
              </div>

              {/* 第 2 行：完整展示模板 API 基础地址（无截断限制，允许划选复制） */}
              {boundTemplate ? (
                <div className="pt-1.5 border-t border-border/40 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs">
                  <div className="flex items-center gap-1.5 min-w-0">
                    <span className="shrink-0 text-[11px] font-medium text-muted-foreground">
                      {t("aiGatewayTemplateBaseUrlLabel", "API base URL")}:
                    </span>
                    <code className="font-mono text-xs text-foreground select-all break-all">
                      {boundTemplate.base_url}
                    </code>
                  </div>
                  {boundTemplate.models_url && boundTemplate.models_url !== boundTemplate.base_url ? (
                    <div className="flex items-center gap-1.5 min-w-0">
                      <span className="shrink-0 text-[11px] font-medium text-muted-foreground">
                        {t("aiGatewayTemplateModelsUrl", "Models URL")}:
                      </span>
                      <code className="font-mono text-xs text-muted-foreground select-all break-all">
                        {boundTemplate.models_url}
                      </code>
                    </div>
                  ) : null}
                </div>
              ) : null}
            </div>
          ) : null}
          {/* 基础配置两列网格（使用 AI 终端服务商统一的标准 field-grid 和 field） */}
          <div className="field-grid col-2 mb-0">
            {/* 第 1 行：名称与自定义图标并排 */}
            <div className="field">
              <label className="required">{t("aiGatewayName", "Name")}</label>
              <input
                type="text"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="e.g. DeepSeek / OpenAI"
                aria-label={t("aiGatewayName", "Name")}
              />
            </div>

            <div className="field">
              <label className="inline-flex items-center gap-1.5">
                <span>{t("aiGatewayCustomIcon", "Provider icon")}</span>
                <span className="relative group inline-flex items-center">
                  <Info
                    className="h-3.5 w-3.5 cursor-help text-muted-foreground/70 transition hover:text-foreground"
                    aria-label={t("aiGatewayCustomIconDesc", "Select a custom icon or inherit from the bound template.")}
                  />
                  <span
                    role="tooltip"
                    className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-56 rounded-md border bg-popover p-2 text-left text-xs font-normal text-popover-foreground shadow-lg group-hover:block group-focus-within:block"
                  >
                    {t(
                      "aiGatewayCustomIconDesc",
                      "Select a custom icon or inherit from the bound template.",
                    )}
                  </span>
                </span>
              </label>
              <ProviderTemplateIconPicker
                value={icon}
                onChange={setIcon}
                options={PROVIDER_CUSTOM_ICON_OPTIONS}
                inheritedIcon={
                  boundTemplate?.icon ||
                  resolveEffectiveProviderIcon(
                    { template_id: provider?.template_id, base_url: baseUrl, name, id: provider?.id },
                    boundTemplate,
                  )
                }
                autoLabel={
                  isTemplateBound
                    ? t("aiGatewayInheritTemplateIcon", "Inherit from template (Default)")
                    : t("aiGatewayDefaultIcon", "Default icon")
                }
                templateId={(boundTemplate?.id ?? provider?.template_id) || undefined}
                templateName={name || boundTemplate?.name}
                baseUrl={baseUrl}
                triggerTestId="provider-edit-icon-trigger"
                selectTestId="provider-edit-icon"
                menuTestId="provider-edit-icon-menu"
              />
            </div>

            {/* 第 2 行：接口协议与默认模型并排 */}
            <div className="field">
              <label className="required">{t("aiGatewayProtocol", "API protocol")}</label>
              <select
                value={protocol}
                onChange={(event) =>
                  setProtocol(event.target.value as GatewayUpstreamProtocol)
                }
                aria-label={t("aiGatewayProtocol", "API protocol")}
              >
                <option value="chat_completions">
                  {t("aiGatewayProtocolChat", "Chat Completions (/chat/completions)")}
                </option>
                <option value="responses">
                  {t("aiGatewayProtocolResponses", "Responses (/responses)")}
                </option>
              </select>
            </div>

            <div className="field">
              <label className="inline-flex items-center gap-1.5">
                <span>{t("aiGatewayDefaultModel", "Default model")}</span>
                <span className="relative group inline-flex items-center">
                  <Info
                    className="h-3.5 w-3.5 cursor-help text-muted-foreground/70 transition hover:text-foreground"
                    aria-label={t("aiGatewayDefaultModelHint", "Enter the real model name on the upstream provider. Used as a fallback when no mapping matches.")}
                  />
                  <span
                    role="tooltip"
                    className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-56 rounded-md border bg-popover p-2 text-left text-xs font-normal text-popover-foreground shadow-lg group-hover:block group-focus-within:block"
                  >
                    {t(
                      "aiGatewayDefaultModelHint",
                      "Enter the real model name on the upstream provider. Used as a fallback when no mapping matches.",
                    )}
                  </span>
                </span>
              </label>
              <select
                data-testid="ai-gateway-default-model-select"
                value={defaultModel}
                onChange={(event) => setDefaultModel(event.target.value)}
                aria-label={t("aiGatewayDefaultModel", "Default model")}
                className="font-mono"
              >
                <option value="">
                  {t("aiGatewayDefaultModelNone", "None")}
                </option>
                {mappedUpstreamModels({ ...provider, mappings }).map((model) => (
                  <option key={model} value={model}>
                    {model}
                  </option>
                ))}
              </select>
            </div>

            {/* 第 3 行：API Base URL 独占一行 */}
            <div className="field full-span">
              <label className="required">{t("aiGatewayBaseUrl", "API base URL")}</label>
              <input
                type="text"
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
                placeholder="https://api.openai.com"
                aria-label={t("aiGatewayBaseUrl", "API base URL")}
                className="font-mono"
              />
            </div>

            {/* 路由权重独占一行 */}
            <div className="field full-span">
              <label className="inline-flex items-center gap-1.5">
                <span>{t("aiGateway.provider.weight", "Weight")}</span>
                <span className="relative group inline-flex items-center">
                  <Info
                    className="h-3.5 w-3.5 cursor-help text-muted-foreground/70 transition hover:text-foreground"
                    aria-label={t(
                      "aiGateway.provider.weightHint",
                      "Higher weight forwards requests more frequently (1-100)",
                    )}
                  />
                  <span
                    role="tooltip"
                    className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-64 rounded-md border bg-popover p-2 text-left text-xs font-normal text-popover-foreground shadow-lg group-hover:block group-focus-within:block"
                  >
                    {t(
                      "aiGateway.provider.weightHint",
                      "Higher weight forwards requests more frequently (1-100)",
                    )}
                  </span>
                </span>
              </label>
              <input
                data-testid="ai-gateway-provider-weight-input"
                type="number"
                min={1}
                max={100}
                step={1}
                value={weight}
                onChange={(event) =>
                  setWeight(
                    event.target.value === "" ? "" : Number(event.target.value),
                  )
                }
                placeholder="1"
                aria-label={t("aiGateway.provider.weight", "Weight")}
              />
              <p className="text-[11px] text-muted-foreground mt-1">
                {t(
                  "aiGateway.provider.weightHint",
                  "Higher weight forwards requests more frequently (1-100)",
                )}
              </p>
            </div>

            {/* 第 4 行：API Key 独占一行 */}
            <div className="field full-span">
              <label>{t("aiGatewayApiKey", "API key")}</label>
              <div className="relative">
                <input
                  type={revealApiKey ? "text" : "password"}
                  value={apiKey}
                  onChange={(event) => setApiKey(event.target.value)}
                  placeholder="sk-..."
                  aria-label={t("aiGatewayApiKey", "API key")}
                  className="pr-10 font-mono"
                />
                <button
                  type="button"
                  onClick={() => setRevealApiKey((prev) => !prev)}
                  aria-label={
                    revealApiKey
                      ? t("aiGatewayHideSecret", "Hide secret")
                      : t("aiGatewayShowSecret", "Show secret")
                  }
                  className="absolute right-1 top-1/2 inline-flex h-8 w-8 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground"
                >
                  {revealApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </button>
              </div>
            </div>

            {/* 第 5 行：标签区块（非必填，支持多选） */}
            <div className="field full-span">
              <label className="inline-flex items-center justify-between w-full">
                <span className="inline-flex items-center gap-1.5">
                  <Tag className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{t("aiGatewayProviderTags", "Tags")}</span>
                  <span className="text-[11px] font-normal text-muted-foreground">
                    ({t("commonOptional", "Optional")})
                  </span>
                </span>
              </label>
              <div
                data-testid="ai-gateway-provider-tags-container"
                className="flex flex-wrap items-center gap-1.5 min-h-[38px] rounded-lg border border-border bg-background p-1.5 focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/50"
              >
                {tags.map((tag) => (
                  <span
                    key={tag}
                    data-testid={`provider-tag-badge-${tag}`}
                    className="inline-flex items-center gap-1 rounded-md bg-primary/10 border border-primary/20 px-2 py-0.5 text-xs font-medium text-primary"
                  >
                    <span>{tag}</span>
                    <button
                      type="button"
                      data-testid={`remove-tag-${tag}`}
                      onClick={() => handleRemoveTag(tag)}
                      aria-label={`Remove tag ${tag}`}
                      className="rounded p-0.5 hover:bg-primary/20 transition-colors"
                    >
                      <X className="h-3 w-3" />
                    </button>
                  </span>
                ))}
                <input
                  data-testid="ai-gateway-provider-tags-input"
                  type="text"
                  value={tagInput}
                  onChange={(e) => setTagInput(e.target.value)}
                  onKeyDown={handleTagInputKeyDown}
                  placeholder={
                    tags.length === 0
                      ? t(
                          "aiGatewayProviderTagsPlaceholder",
                          "Add a tag and press Enter...",
                        )
                      : ""
                  }
                  aria-label={t("aiGatewayProviderTags", "Tags")}
                  className="flex-1 min-w-[140px] bg-transparent border-none outline-none text-sm text-foreground placeholder:text-muted-foreground/60 h-6 px-1"
                />
              </div>

              {candidateTags.length > 0 && (
                <div className="mt-1.5 flex flex-wrap items-center gap-1">
                  <span className="text-[11px] text-muted-foreground mr-1">
                    {t("aiGatewayRecommendedTags", "Suggested tags")}:
                  </span>
                  {candidateTags.map((candidate) => {
                    const isSelected = tags.includes(candidate);
                    return (
                      <button
                        key={candidate}
                        type="button"
                        data-testid={`suggested-tag-${candidate}`}
                        onClick={() => handleToggleTag(candidate)}
                        className={`inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] font-medium transition-all ${
                          isSelected
                            ? "bg-primary text-primary-foreground shadow-2xs"
                            : "bg-muted/70 text-muted-foreground hover:bg-muted hover:text-foreground border border-border/50"
                        }`}
                      >
                        {isSelected ? <Check className="h-3 w-3" /> : <Plus className="h-3 w-3 opacity-60" />}
                        <span>{candidate}</span>
                      </button>
                    );
                  })}
                </div>
              )}
              <p className="text-[11px] text-muted-foreground mt-1">
                {t(
                  "aiGatewayProviderTagsDesc",
                  "Assign tags to categorize providers for filtering (optional).",
                )}
              </p>
            </div>
          </div>

          {/* 模型映射列表（宽幅舒展设计，输入框尺寸与标准 field 保持一致） */}
          <div className="space-y-2 rounded-xl border bg-muted/20 p-3.5">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-xs font-semibold text-foreground">
                    {t("aiGatewayModelMappings", "Model mappings")}
                  </span>
                  <span
                    data-testid="ai-gateway-mappings-count-badge"
                    className="inline-flex items-center rounded-full bg-background px-2 py-0.5 text-[10px] font-semibold text-muted-foreground border border-border/70 shadow-2xs"
                  >
                    {t("aiGatewayConfiguredModelCount", {
                      count: mappings.length,
                      defaultValue: `${mappings.length} configured`,
                    })}
                  </span>
                </div>
                <p className="text-[11px] text-muted-foreground mt-0.5">
                  {t(
                    "aiGatewayModelMappingsDesc",
                    "Map local request model names to upstream models. Optionally set a display name shown in the gateway for each model.",
                  )}
                </p>
              </div>
              <div className="flex items-center gap-2">
                {mappings.length > 0 ? (
                  <>
                    <button
                      type="button"
                      data-testid="ai-gateway-enable-all-mappings"
                      onClick={() =>
                        setMappings((prev) =>
                          prev.map((entry) => ({ ...entry, enabled: true })),
                        )
                      }
                      disabled={
                        busy || mappings.every((entry) => entry.enabled !== false)
                      }
                      aria-label={t("aiGatewayEnableAllMappings", "Enable all")}
                      className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2 text-xs font-medium shadow-sm transition hover:bg-muted disabled:opacity-50"
                    >
                      <Eye className="h-3 w-3" />
                      {t("aiGatewayEnableAllMappings", "Enable all")}
                    </button>
                    <button
                      type="button"
                      data-testid="ai-gateway-disable-all-mappings"
                      onClick={() =>
                        setMappings((prev) =>
                          prev.map((entry) => ({ ...entry, enabled: false })),
                        )
                      }
                      disabled={
                        busy || mappings.every((entry) => entry.enabled === false)
                      }
                      aria-label={t("aiGatewayDisableAllMappings", "Disable all")}
                      className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2 text-xs font-medium shadow-sm transition hover:bg-muted disabled:opacity-50"
                    >
                      <EyeOff className="h-3 w-3" />
                      {t("aiGatewayDisableAllMappings", "Disable all")}
                    </button>
                  </>
                ) : null}
                {autoDisabledModels.length > 0 && provider.id !== "" && onReenableModels ? (
                  <button
                    type="button"
                    data-testid={`ai-gateway-reenable-models-${provider.id}`}
                    onClick={() => onReenableModels(provider.id)}
                    disabled={busy}
                    className="inline-flex h-7 items-center gap-1.5 rounded-md border border-amber-500/40 bg-background px-2 text-xs font-medium text-amber-700 shadow-sm transition hover:bg-amber-500/15 disabled:opacity-50 dark:text-amber-400"
                  >
                    <RotateCcw className="h-3 w-3" />
                    {t("aiGatewayReenableAllMappings", "Re-enable all mappings")}
                  </button>
                ) : null}
                <button
                  type="button"
                  onClick={() =>
                    setMappings((prev) => [
                      ...prev,
                      {
                        local_model: "",
                        upstream_model: "",
                        display_name: "",
                        protocol: null,
                        enabled: true,
                      },
                    ])
                  }
                  className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2 text-xs font-medium shadow-sm transition hover:bg-muted"
                >
                  <Plus className="h-3 w-3" />
                  {t("aiGatewayAddMapping", "Add mapping")}
                </button>
              </div>
            </div>

            {mappings.length === 0 ? (
              <p className="rounded-lg border border-dashed bg-background/50 px-3 py-3 text-center text-xs text-muted-foreground">
                {t("aiGatewayNoMappings", "No model mappings configured.")}
              </p>
            ) : (
              <div className="overflow-x-auto">
                <ul className="space-y-2">
                {mappings.map((mapping, index) => {
                  const deprecated = Boolean(
                    boundTemplate && isMappingDeprecated(mapping, boundTemplate),
                  );
                  const isExpanded = expandedMappings[index] === true;
                  const efforts = mapping.reasoning_efforts ?? [];
                  const upstreamModel = mapping.upstream_model.trim();
                  const isAutoAdded =
                    upstreamModel !== "" && upstreamModel === autoAddedModel;
                  const isAutoDisabled = mapping.auto_disabled === true;
                  const isEffectiveEnabled =
                    mapping.enabled !== false && !isAutoDisabled;
                  return (
                  <li
                    key={index}
                    data-disabled={mapping.enabled === false ? "true" : undefined}
                    data-auto-disabled={isAutoDisabled ? "true" : undefined}
                    data-deprecated={deprecated ? "true" : undefined}
                    data-auto-added={isAutoAdded ? "true" : undefined}
                    className={`space-y-2 rounded-lg ${
                      mapping.enabled === false ? "opacity-60" : ""
                    } ${
                      isAutoDisabled
                        ? "border border-amber-500/40 bg-amber-500/5 px-2 py-1.5"
                        : ""
                    }`}
                  >
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        data-testid={`ai-gateway-mapping-expand-${index}`}
                        aria-expanded={isExpanded}
                        aria-label={t("aiGatewayMappingDetails", "Mapping details")}
                        onClick={() =>
                          setExpandedMappings((prev) => ({
                            ...prev,
                            [index]: !prev[index],
                          }))
                        }
                        className="inline-flex h-[38px] w-[30px] shrink-0 items-center justify-center rounded-lg text-muted-foreground transition hover:bg-muted hover:text-foreground"
                      >
                        {isExpanded ? (
                          <ChevronUp className="h-4 w-4" />
                        ) : (
                          <ChevronDown className="h-4 w-4" />
                        )}
                      </button>
                      <Switch
                        aria-label={t("aiGatewayToggleMappingAria", {
                          index: index + 1,
                          defaultValue: `Enable mapping ${index + 1}`,
                        })}
                        checked={isEffectiveEnabled}
                        onCheckedChange={(checked) => {
                          if (checked) {
                            updateMapping(index, {
                              enabled: true,
                              auto_disabled: false,
                            });
                            if (
                              isAutoDisabled &&
                              provider.id !== "" &&
                              onReenableModel
                            ) {
                              onReenableModel(
                                provider.id,
                                mapping.local_model,
                                mapping.upstream_model,
                              );
                            }
                          } else {
                            updateMapping(index, { enabled: false });
                          }
                        }}
                      />
                      <input
                        type="text"
                        value={mapping.local_model}
                        onChange={(event) =>
                          updateMapping(index, { local_model: event.target.value })
                        }
                        placeholder={t("aiGatewayLocalModelPlaceholder", "local model")}
                        aria-label={t("aiGatewayLocalModelAria", {
                          index: index + 1,
                          defaultValue: `Local model ${index + 1}`,
                        })}
                        className={`${mappingInputClass} min-w-[120px] flex-1 font-mono`}
                      />
                      <span aria-hidden="true" className="shrink-0 text-muted-foreground font-semibold text-sm">
                        →
                      </span>
                      <input
                        type="text"
                        value={mapping.upstream_model}
                        onChange={(event) =>
                          updateMapping(index, { upstream_model: event.target.value })
                        }
                        placeholder={t("aiGatewayUpstreamModelPlaceholder", "upstream model")}
                        aria-label={t("aiGatewayUpstreamModelAria", {
                          index: index + 1,
                          defaultValue: `Upstream model ${index + 1}`,
                        })}
                        className={`${mappingInputClass} min-w-[120px] flex-1 font-mono`}
                      />
                      {deprecated ? (
                        <span className="shrink-0 rounded-full border border-amber-500/40 bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400">
                          {t("aiGatewayTemplateDeprecated")}
                        </span>
                      ) : null}
                      <input
                        type="text"
                        value={mapping.display_name ?? ""}
                        onChange={(event) =>
                          updateMapping(index, { display_name: event.target.value })
                        }
                        placeholder={t("aiGatewayLocalModelNamePlaceholder", "display name")}
                        aria-label={t("aiGatewayLocalModelNameAria", {
                          index: index + 1,
                          defaultValue: `Local model name ${index + 1}`,
                        })}
                        className={`${mappingInputClass} min-w-[120px] flex-1`}
                      />
                      <select
                        value={mapping.protocol ?? ""}
                        onChange={(event) =>
                          updateMapping(index, {
                            protocol:
                              event.target.value === ""
                                ? null
                                : (event.target.value as GatewayUpstreamProtocol),
                          })
                        }
                        aria-label={t("aiGatewayMappingProtocolAria", {
                          index: index + 1,
                          defaultValue: `Mapping protocol ${index + 1}`,
                        })}
                        className={`${mappingInputClass} min-w-[180px] shrink-0`}
                      >
                        <option value="">
                          {t("aiGatewayProtocolInherit", "Inherit from provider")}
                        </option>
                        <option value="chat_completions">
                          {t("aiGatewayProtocolChat", "Chat Completions (/chat/completions)")}
                        </option>
                        <option value="responses">
                          {t("aiGatewayProtocolResponses", "Responses (/responses)")}
                        </option>
                      </select>
                      <button
                        type="button"
                        onClick={() => handleRemoveMapping(index)}
                        aria-label={t("aiGatewayRemoveMappingAria", {
                          index: index + 1,
                          defaultValue: `Remove mapping ${index + 1}`,
                        })}
                        className="inline-flex h-[38px] w-[38px] shrink-0 items-center justify-center rounded-lg text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                      {isAutoDisabled && provider.id !== "" ? (
                        <button
                          type="button"
                          data-testid={`ai-gateway-reenable-mapping-${mapping.local_model}`}
                          onClick={() =>
                            onReenableModel?.(
                              provider.id,
                              mapping.local_model,
                              mapping.upstream_model,
                            )
                          }
                          disabled={busy}
                          className="inline-flex h-[38px] shrink-0 items-center gap-1.5 rounded-lg border border-amber-500/40 bg-background px-2.5 text-xs font-medium text-amber-700 shadow-sm transition hover:bg-amber-500/15 disabled:opacity-50 dark:text-amber-400"
                        >
                          <RotateCcw className="h-3.5 w-3.5" />
                          {t("aiGatewayReenableMapping", "Re-enable mapping")}
                        </button>
                      ) : null}
                    </div>

                    {isAutoAdded ? (
                      <span className="inline-flex w-fit items-center rounded-full border border-primary/40 bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                        {t("aiGatewayDefaultModelAutoAdded")}
                      </span>
                    ) : null}

                    {isExpanded ? (
                      <>
                        {upstreamModel !== "" ? (
                          <MappingPriceEditor
                            index={index}
                            draft={draftForModel(upstreamModel)}
                            onChange={(patch) =>
                              updateDraftForModel(upstreamModel, patch)
                            }
                          />
                        ) : null}
                      <div
                        data-testid={`ai-gateway-mapping-efforts-${index}`}
                        className="space-y-2 rounded-lg border-t border-border/60 bg-muted/10 px-3 py-2.5"
                      >
                        <div className="text-[11px] font-semibold text-foreground">
                          {t("aiGatewayReasoningEfforts", "Reasoning efforts")}
                        </div>
                        <p className="text-[11px] text-muted-foreground">
                          {t(
                            "aiGatewayReasoningEffortsDesc",
                            "Add or remove the reasoning-effort identifiers this model advertises.",
                          )}
                        </p>
                        {efforts.length > 0 ? (
                          <div className="flex flex-wrap items-center gap-1.5">
                            {efforts.map((effort) => (
                              <span
                                key={effort}
                                data-testid={`ai-gateway-mapping-effort-${index}-${effort}`}
                                className="inline-flex items-center gap-1 rounded-full border border-border bg-secondary px-2 py-0.5 text-[11px] font-medium text-secondary-foreground"
                              >
                                <span className="font-mono">{effort}</span>
                                <button
                                  type="button"
                                  data-testid={`ai-gateway-mapping-effort-remove-${index}-${effort}`}
                                  aria-label={t("aiGatewayReasoningEffortRemove", {
                                    effort,
                                    defaultValue: `Remove ${effort}`,
                                  })}
                                  onClick={() => removeEffort(index, effort)}
                                  className="inline-flex h-4 w-4 items-center justify-center rounded-full text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                                >
                                  <X className="h-3 w-3" />
                                </button>
                              </span>
                            ))}
                          </div>
                        ) : null}
                        <div className="flex items-center gap-2">
                          <input
                            type="text"
                            data-testid={`ai-gateway-mapping-effort-input-${index}`}
                            value={effortInputs[index] ?? ""}
                            onChange={(event) =>
                              setEffortInputs((prev) => ({
                                ...prev,
                                [index]: event.target.value,
                              }))
                            }
                            onKeyDown={(event) => {
                              if (event.key === "Enter") {
                                event.preventDefault();
                                addEffort(index);
                              }
                            }}
                            placeholder={t(
                              "aiGatewayReasoningEffortPlaceholder",
                              "e.g. high",
                            )}
                            aria-label={t("aiGatewayReasoningEfforts", "Reasoning efforts")}
                            className={`${mappingInputClass} h-8 min-w-[140px] flex-1 font-mono`}
                          />
                          <button
                            type="button"
                            data-testid={`ai-gateway-mapping-effort-add-${index}`}
                            onClick={() => addEffort(index)}
                            className="inline-flex h-8 items-center gap-1.5 rounded-md border bg-background px-2.5 text-xs font-medium shadow-sm transition hover:bg-muted"
                          >
                            <Plus className="h-3 w-3" />
                            {t("aiGatewayReasoningEffortAdd", "Add")}
                          </button>
                        </div>
                      </div>
                      </>
                    ) : null}
                  </li>
                  );
                })}
                </ul>
              </div>
            )}

            {ignoredModels.length > 0 ? (
              <div
                data-testid="ai-gateway-ignored-models"
                className="rounded-lg border border-border/70 bg-background/60 p-2.5"
              >
                <div className="flex items-center gap-1.5">
                  <ArchiveRestore className="h-3.5 w-3.5 text-muted-foreground" />
                  <span className="text-[11px] font-semibold text-foreground">
                    {t("aiGatewayIgnoredModels", "Ignored models")}
                  </span>
                </div>
                <p className="mt-0.5 text-[11px] text-muted-foreground">
                  {t(
                    "aiGatewayIgnoredModelsDesc",
                    "Models you removed from this template. Restore one to rebuild it from the template's current data.",
                  )}
                </p>
                <ul className="mt-1.5 space-y-1">
                  {ignoredModels.map((model) => (
                    <li
                      key={model}
                      data-testid={`ai-gateway-ignored-model-${model}`}
                      className="flex items-center justify-between gap-2 rounded-md bg-muted/40 px-2 py-1"
                    >
                      <span className="truncate font-mono text-[11px] text-foreground">
                        {model}
                      </span>
                      <button
                        type="button"
                        data-testid={`ai-gateway-restore-model-${model}`}
                        onClick={() => onRestoreModel?.(provider.id, model)}
                        className="inline-flex h-6 shrink-0 items-center gap-1 rounded-md border bg-background px-2 text-[11px] font-medium text-muted-foreground transition hover:bg-muted hover:text-foreground"
                      >
                        <RotateCcw className="h-3 w-3" />
                        {t("aiGatewayRestoreModel", "Restore")}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
          </div>
        </div>

        <DialogFooter className="px-6 py-4 border-t bg-card/80 backdrop-blur-sm shrink-0 flex flex-row items-center justify-between gap-2 sm:justify-between">
          <div>
            {isEditing && onDelete ? (
              <button
                type="button"
                onClick={handleDelete}
                disabled={busy}
                className="acc-panel-btn danger"
              >
                <Trash2 className="h-4 w-4" />
                {t("aiGatewayDelete", "Delete")}
              </button>
            ) : null}
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              disabled={busy}
              className="acc-panel-btn"
            >
              {t("cancel", "Cancel")}
            </button>
            <button
              type="button"
              onClick={handleSave}
              disabled={busy || !name.trim() || !baseUrl.trim()}
              className="acc-panel-btn primary"
            >
              {t("aiGatewaySave", "Save")}
            </button>
          </div>
        </DialogFooter>

        {confirmDeleting && (
          <div
            data-testid="ai-gateway-delete-provider-confirm-dialog"
            className="absolute inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm p-4 animate-in fade-in-0 duration-150"
          >
            <div className="bg-card border rounded-xl shadow-lg w-full max-w-sm overflow-hidden animate-in zoom-in-95 duration-150">
              <div className="p-5">
                <div className="flex items-center gap-3 mb-3 text-destructive">
                  <div className="bg-destructive/10 p-2 rounded-full">
                    <AlertCircle className="w-5 h-5" />
                  </div>
                  <h3 className="font-semibold text-foreground">
                    {t("aiGatewayDeleteProviderTitle", "Delete Provider")}
                  </h3>
                </div>
                <p className="text-sm text-muted-foreground whitespace-pre-wrap break-words">
                  {t(
                    "aiGatewayDeleteProviderConfirm",
                    'Are you sure you want to delete upstream provider "{{name}}"? This action cannot be undone.',
                    { name: name.trim() || provider.name || provider.id },
                  )}
                </p>
              </div>
              <div className="p-4 bg-muted/30 border-t flex justify-end gap-3">
                <button
                  type="button"
                  data-testid="ai-gateway-delete-provider-cancel"
                  onClick={handleCancelDelete}
                  disabled={busy}
                  className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors text-foreground"
                >
                  {t("cancel", "Cancel")}
                </button>
                <button
                  type="button"
                  data-testid="ai-gateway-delete-provider-confirm"
                  onClick={handleConfirmDelete}
                  disabled={busy}
                  className="px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors bg-destructive text-destructive-foreground hover:bg-destructive/90"
                >
                  <Trash2 className="h-4 w-4" />
                  {t("delete", "Delete")}
                </button>
              </div>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
