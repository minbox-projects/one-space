import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ArchiveRestore,
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
} from "@/lib/apiGateway";
import { MappingPriceEditor } from "./MappingPriceEditor";
import { ProviderTemplateAvatar } from "./ProviderTemplateIcon";

type ProviderDetailDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  provider: GatewayUpstreamProvider | null;
  prices?: ModelPrice[];
  busy: boolean;
  onSave: (provider: GatewayUpstreamProvider, prices: ModelPrice[]) => void;
  onDelete?: (providerId: string) => void;
  templates?: GatewayProviderTemplateView[];
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

export function ProviderDetailDialog({
  open,
  onOpenChange,
  provider,
  prices,
  busy,
  onSave,
  onDelete,
  templates,
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

  useEffect(() => {
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [provider, open]);

  if (!provider) return null;

  const isEditing = Boolean(provider.id);
  const isTemplateBound = Boolean(provider.template_id);
  const boundTemplateView = provider.template_id
    ? templates?.find((view) => view.template.id === provider.template_id)
    : undefined;
  const boundTemplate = boundTemplateView?.template;
  const lastSyncText = boundTemplateView?.synced_at
    ? formatGatewayTimestamp(boundTemplateView.synced_at)
    : t("apiGatewayTemplateNotSynced", "Not synced yet");
  const ignoredModels = isTemplateBound ? provider.ignored_models ?? [] : [];
  const autoDisabledModels = (provider.mappings ?? []).filter(
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
      },
      submittedPrices,
    );
    onOpenChange(false);
  };

  const handleDelete = () => {
    if (!provider.id || !onDelete) return;
    onDelete(provider.id);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[90vh] w-full sm:max-w-6xl overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0"
        data-testid="api-gateway-provider-detail"
      >
        <DialogHeader className="pl-6 pr-14 py-4 border-b bg-card/80 backdrop-blur-sm shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            {isTemplateBound ? (
              <ProviderTemplateAvatar
                icon={boundTemplate?.icon}
                templateId={boundTemplate?.id ?? provider.template_id}
                templateName={boundTemplate?.name}
                size={36}
                className="shrink-0"
              />
            ) : (
              <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-2xs">
                <Server className="h-4.5 w-4.5" />
              </div>
            )}
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <DialogTitle className="truncate text-base font-semibold leading-5 text-foreground">
                  {isEditing
                    ? t("apiGatewayEditProvider", "Edit provider")
                    : t("apiGatewayNewProvider", "New provider")}
                </DialogTitle>
                {isTemplateBound ? (
                  <span
                    data-testid="api-gateway-bound-template-header-badge"
                    className="inline-flex items-center gap-1 rounded-full bg-emerald-500/10 px-2.5 py-0.5 text-[11px] font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20"
                  >
                    <Sparkles className="h-3 w-3" />
                    <span>
                      {boundTemplate
                        ? t("apiGatewayBoundTemplateBadge", {
                            name: boundTemplate.name,
                            defaultValue: `Template: ${boundTemplate.name}`,
                          })
                        : t("apiGatewayBoundTemplateNotFound", {
                            id: provider.template_id,
                            defaultValue: `Template definition not found (${provider.template_id})`,
                          })}
                    </span>
                  </span>
                ) : null}
              </div>
              <DialogDescription className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                {t(
                  "apiGatewayProviderDialogDesc",
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
              data-testid="api-gateway-bound-template-banner"
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
                      t("apiGatewayBoundTemplateNotFound", {
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
                      {t("apiGatewayBoundTemplatePresetModels", {
                        count: boundTemplate.models.length,
                        defaultValue: `${boundTemplate.models.length} preset models`,
                      })}
                    </span>
                  ) : null}
                </div>

                {boundTemplate ? (
                  <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground shrink-0">
                    <Clock className="h-3 w-3 opacity-60" />
                    <span>{t("apiGatewayTemplateLastSync", "Last sync")}:</span>
                    <span className="font-medium text-foreground">{lastSyncText}</span>
                  </div>
                ) : null}
              </div>

              {/* 第 2 行：完整展示模板 API 基础地址（无截断限制，允许划选复制） */}
              {boundTemplate ? (
                <div className="pt-1.5 border-t border-border/40 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs">
                  <div className="flex items-center gap-1.5 min-w-0">
                    <span className="shrink-0 text-[11px] font-medium text-muted-foreground">
                      {t("apiGatewayTemplateBaseUrlLabel", "API base URL")}:
                    </span>
                    <code className="font-mono text-xs text-foreground select-all break-all">
                      {boundTemplate.base_url}
                    </code>
                  </div>
                  {boundTemplate.models_url && boundTemplate.models_url !== boundTemplate.base_url ? (
                    <div className="flex items-center gap-1.5 min-w-0">
                      <span className="shrink-0 text-[11px] font-medium text-muted-foreground">
                        {t("apiGatewayTemplateModelsUrl", "Models URL")}:
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
            {/* 第 1 行：名称独占一行 */}
            <div className="field full-span">
              <label className="required">{t("apiGatewayName", "Name")}</label>
              <input
                type="text"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="e.g. DeepSeek / OpenAI"
                aria-label={t("apiGatewayName", "Name")}
              />
            </div>

            {/* 第 2 行：接口协议与默认模型并排 */}
            <div className="field">
              <label className="required">{t("apiGatewayProtocol", "API protocol")}</label>
              <select
                value={protocol}
                onChange={(event) =>
                  setProtocol(event.target.value as GatewayUpstreamProtocol)
                }
                aria-label={t("apiGatewayProtocol", "API protocol")}
              >
                <option value="chat_completions">
                  {t("apiGatewayProtocolChat", "Chat Completions (/chat/completions)")}
                </option>
                <option value="responses">
                  {t("apiGatewayProtocolResponses", "Responses (/responses)")}
                </option>
              </select>
            </div>

            <div className="field">
              <label className="inline-flex items-center gap-1.5">
                <span>{t("apiGatewayDefaultModel", "Default model")}</span>
                <span className="relative group inline-flex items-center">
                  <Info
                    className="h-3.5 w-3.5 cursor-help text-muted-foreground/70 transition hover:text-foreground"
                    aria-label={t("apiGatewayDefaultModelHint", "Enter the real model name on the upstream provider. Used as a fallback when no mapping matches.")}
                  />
                  <span
                    role="tooltip"
                    className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-56 rounded-md border bg-popover p-2 text-left text-xs font-normal text-popover-foreground shadow-lg group-hover:block group-focus-within:block"
                  >
                    {t(
                      "apiGatewayDefaultModelHint",
                      "Enter the real model name on the upstream provider. Used as a fallback when no mapping matches.",
                    )}
                  </span>
                </span>
              </label>
              <select
                data-testid="api-gateway-default-model-select"
                value={defaultModel}
                onChange={(event) => setDefaultModel(event.target.value)}
                aria-label={t("apiGatewayDefaultModel", "Default model")}
                className="font-mono"
              >
                <option value="">
                  {t("apiGatewayDefaultModelNone", "None")}
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
              <label className="required">{t("apiGatewayBaseUrl", "API base URL")}</label>
              <input
                type="text"
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
                placeholder="https://api.openai.com"
                aria-label={t("apiGatewayBaseUrl", "API base URL")}
                className="font-mono"
              />
            </div>

            {/* 路由权重独占一行 */}
            <div className="field full-span">
              <label className="inline-flex items-center gap-1.5">
                <span>{t("apiGateway.provider.weight", "Weight")}</span>
                <span className="relative group inline-flex items-center">
                  <Info
                    className="h-3.5 w-3.5 cursor-help text-muted-foreground/70 transition hover:text-foreground"
                    aria-label={t(
                      "apiGateway.provider.weightHint",
                      "Higher weight forwards requests more frequently (1-100)",
                    )}
                  />
                  <span
                    role="tooltip"
                    className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-64 rounded-md border bg-popover p-2 text-left text-xs font-normal text-popover-foreground shadow-lg group-hover:block group-focus-within:block"
                  >
                    {t(
                      "apiGateway.provider.weightHint",
                      "Higher weight forwards requests more frequently (1-100)",
                    )}
                  </span>
                </span>
              </label>
              <input
                data-testid="api-gateway-provider-weight-input"
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
                aria-label={t("apiGateway.provider.weight", "Weight")}
              />
              <p className="text-[11px] text-muted-foreground mt-1">
                {t(
                  "apiGateway.provider.weightHint",
                  "Higher weight forwards requests more frequently (1-100)",
                )}
              </p>
            </div>

            {/* 第 4 行：API Key 独占一行 */}
            <div className="field full-span">
              <label>{t("apiGatewayApiKey", "API key")}</label>
              <div className="relative">
                <input
                  type={revealApiKey ? "text" : "password"}
                  value={apiKey}
                  onChange={(event) => setApiKey(event.target.value)}
                  placeholder="sk-..."
                  aria-label={t("apiGatewayApiKey", "API key")}
                  className="pr-10 font-mono"
                />
                <button
                  type="button"
                  onClick={() => setRevealApiKey((prev) => !prev)}
                  aria-label={
                    revealApiKey
                      ? t("apiGatewayHideSecret", "Hide secret")
                      : t("apiGatewayShowSecret", "Show secret")
                  }
                  className="absolute right-1 top-1/2 inline-flex h-8 w-8 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground"
                >
                  {revealApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </button>
              </div>
            </div>
          </div>

          {/* 模型映射列表（宽幅舒展设计，输入框尺寸与标准 field 保持一致） */}
          <div className="space-y-2 rounded-xl border bg-muted/20 p-3.5">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-xs font-semibold text-foreground">
                    {t("apiGatewayModelMappings", "Model mappings")}
                  </span>
                  <span
                    data-testid="api-gateway-mappings-count-badge"
                    className="inline-flex items-center rounded-full bg-background px-2 py-0.5 text-[10px] font-semibold text-muted-foreground border border-border/70 shadow-2xs"
                  >
                    {t("apiGatewayConfiguredModelCount", {
                      count: mappings.length,
                      defaultValue: `${mappings.length} configured`,
                    })}
                  </span>
                </div>
                <p className="text-[11px] text-muted-foreground mt-0.5">
                  {t(
                    "apiGatewayModelMappingsDesc",
                    "Map local request model names to upstream models. Optionally set a display name shown in the gateway for each model.",
                  )}
                </p>
              </div>
              <div className="flex items-center gap-2">
                {autoDisabledModels.length > 0 && provider.id !== "" && onReenableModels ? (
                  <button
                    type="button"
                    data-testid={`api-gateway-reenable-models-${provider.id}`}
                    onClick={() => onReenableModels(provider.id)}
                    disabled={busy}
                    className="inline-flex h-7 items-center gap-1.5 rounded-md border border-amber-500/40 bg-background px-2 text-xs font-medium text-amber-700 shadow-sm transition hover:bg-amber-500/15 disabled:opacity-50 dark:text-amber-400"
                  >
                    <RotateCcw className="h-3 w-3" />
                    {t("apiGatewayReenableAllMappings", "Re-enable all mappings")}
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
                  {t("apiGatewayAddMapping", "Add mapping")}
                </button>
              </div>
            </div>

            {mappings.length === 0 ? (
              <p className="rounded-lg border border-dashed bg-background/50 px-3 py-3 text-center text-xs text-muted-foreground">
                {t("apiGatewayNoMappings", "No model mappings configured.")}
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
                        data-testid={`api-gateway-mapping-expand-${index}`}
                        aria-expanded={isExpanded}
                        aria-label={t("apiGatewayMappingDetails", "Mapping details")}
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
                        aria-label={t("apiGatewayToggleMappingAria", {
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
                        placeholder={t("apiGatewayLocalModelPlaceholder", "local model")}
                        aria-label={t("apiGatewayLocalModelAria", {
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
                        placeholder={t("apiGatewayUpstreamModelPlaceholder", "upstream model")}
                        aria-label={t("apiGatewayUpstreamModelAria", {
                          index: index + 1,
                          defaultValue: `Upstream model ${index + 1}`,
                        })}
                        className={`${mappingInputClass} min-w-[120px] flex-1 font-mono`}
                      />
                      {deprecated ? (
                        <span className="shrink-0 rounded-full border border-amber-500/40 bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400">
                          {t("apiGatewayTemplateDeprecated")}
                        </span>
                      ) : null}
                      <input
                        type="text"
                        value={mapping.display_name ?? ""}
                        onChange={(event) =>
                          updateMapping(index, { display_name: event.target.value })
                        }
                        placeholder={t("apiGatewayLocalModelNamePlaceholder", "display name")}
                        aria-label={t("apiGatewayLocalModelNameAria", {
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
                        aria-label={t("apiGatewayMappingProtocolAria", {
                          index: index + 1,
                          defaultValue: `Mapping protocol ${index + 1}`,
                        })}
                        className={`${mappingInputClass} min-w-[180px] shrink-0`}
                      >
                        <option value="">
                          {t("apiGatewayProtocolInherit", "Inherit from provider")}
                        </option>
                        <option value="chat_completions">
                          {t("apiGatewayProtocolChat", "Chat Completions (/chat/completions)")}
                        </option>
                        <option value="responses">
                          {t("apiGatewayProtocolResponses", "Responses (/responses)")}
                        </option>
                      </select>
                      <button
                        type="button"
                        onClick={() => handleRemoveMapping(index)}
                        aria-label={t("apiGatewayRemoveMappingAria", {
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
                          data-testid={`api-gateway-reenable-mapping-${mapping.local_model}`}
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
                          {t("apiGatewayReenableMapping", "Re-enable mapping")}
                        </button>
                      ) : null}
                    </div>

                    {isAutoAdded ? (
                      <span className="inline-flex w-fit items-center rounded-full border border-primary/40 bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                        {t("apiGatewayDefaultModelAutoAdded")}
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
                        data-testid={`api-gateway-mapping-efforts-${index}`}
                        className="space-y-2 rounded-lg border-t border-border/60 bg-muted/10 px-3 py-2.5"
                      >
                        <div className="text-[11px] font-semibold text-foreground">
                          {t("apiGatewayReasoningEfforts", "Reasoning efforts")}
                        </div>
                        <p className="text-[11px] text-muted-foreground">
                          {t(
                            "apiGatewayReasoningEffortsDesc",
                            "Add or remove the reasoning-effort identifiers this model advertises.",
                          )}
                        </p>
                        {efforts.length > 0 ? (
                          <div className="flex flex-wrap items-center gap-1.5">
                            {efforts.map((effort) => (
                              <span
                                key={effort}
                                data-testid={`api-gateway-mapping-effort-${index}-${effort}`}
                                className="inline-flex items-center gap-1 rounded-full border border-border bg-secondary px-2 py-0.5 text-[11px] font-medium text-secondary-foreground"
                              >
                                <span className="font-mono">{effort}</span>
                                <button
                                  type="button"
                                  data-testid={`api-gateway-mapping-effort-remove-${index}-${effort}`}
                                  aria-label={t("apiGatewayReasoningEffortRemove", {
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
                            data-testid={`api-gateway-mapping-effort-input-${index}`}
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
                              "apiGatewayReasoningEffortPlaceholder",
                              "e.g. high",
                            )}
                            aria-label={t("apiGatewayReasoningEfforts", "Reasoning efforts")}
                            className={`${mappingInputClass} h-8 min-w-[140px] flex-1 font-mono`}
                          />
                          <button
                            type="button"
                            data-testid={`api-gateway-mapping-effort-add-${index}`}
                            onClick={() => addEffort(index)}
                            className="inline-flex h-8 items-center gap-1.5 rounded-md border bg-background px-2.5 text-xs font-medium shadow-sm transition hover:bg-muted"
                          >
                            <Plus className="h-3 w-3" />
                            {t("apiGatewayReasoningEffortAdd", "Add")}
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
                data-testid="api-gateway-ignored-models"
                className="rounded-lg border border-border/70 bg-background/60 p-2.5"
              >
                <div className="flex items-center gap-1.5">
                  <ArchiveRestore className="h-3.5 w-3.5 text-muted-foreground" />
                  <span className="text-[11px] font-semibold text-foreground">
                    {t("apiGatewayIgnoredModels", "Ignored models")}
                  </span>
                </div>
                <p className="mt-0.5 text-[11px] text-muted-foreground">
                  {t(
                    "apiGatewayIgnoredModelsDesc",
                    "Models you removed from this template. Restore one to rebuild it from the template's current data.",
                  )}
                </p>
                <ul className="mt-1.5 space-y-1">
                  {ignoredModels.map((model) => (
                    <li
                      key={model}
                      data-testid={`api-gateway-ignored-model-${model}`}
                      className="flex items-center justify-between gap-2 rounded-md bg-muted/40 px-2 py-1"
                    >
                      <span className="truncate font-mono text-[11px] text-foreground">
                        {model}
                      </span>
                      <button
                        type="button"
                        data-testid={`api-gateway-restore-model-${model}`}
                        onClick={() => onRestoreModel?.(provider.id, model)}
                        className="inline-flex h-6 shrink-0 items-center gap-1 rounded-md border bg-background px-2 text-[11px] font-medium text-muted-foreground transition hover:bg-muted hover:text-foreground"
                      >
                        <RotateCcw className="h-3 w-3" />
                        {t("apiGatewayRestoreModel", "Restore")}
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
                {t("apiGatewayDelete", "Delete")}
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
              {t("apiGatewaySave", "Save")}
            </button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
