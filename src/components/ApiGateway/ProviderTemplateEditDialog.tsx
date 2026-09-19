import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertCircle,
  ChevronDown,
  ChevronUp,
  Plus,
  Search,
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
  normalizeReasoningEfforts,
  priceRowToDraft,
  type GatewayPriceDraft,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateModel,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
  type ModelPrice,
} from "@/lib/apiGateway";
import { MappingPriceEditor } from "./MappingPriceEditor";

export type ProviderTemplateEditDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  template: GatewayProviderTemplate | null;
  providers: GatewayUpstreamProvider[];
  busy: boolean;
  onSave: (template: GatewayProviderTemplate) => Promise<boolean>;
  onDelete: (templateId: string) => Promise<boolean>;
};

const mappingInputClass =
  "h-[38px] rounded-lg border border-border bg-background px-3 text-sm text-foreground outline-none transition-all focus:border-primary focus:ring-2 focus:ring-primary/50 disabled:opacity-50 disabled:cursor-not-allowed";

export function ProviderTemplateEditDialog({
  open,
  onOpenChange,
  template,
  providers,
  busy,
  onSave,
  onDelete,
}: ProviderTemplateEditDialogProps) {
  const { t } = useTranslation();
  const isEditing = Boolean(template?.id);

  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [protocol, setProtocol] =
    useState<GatewayUpstreamProtocol>("chat_completions");
  const [modelsUrl, setModelsUrl] = useState("");
  const [description, setDescription] = useState("");
  const [models, setModels] = useState<GatewayProviderTemplateModel[]>([]);
  const [priceDrafts, setPriceDrafts] = useState<GatewayPriceDraft[]>([]);
  const [expandedModels, setExpandedModels] = useState<Record<number, boolean>>(
    {},
  );
  const [effortInputs, setEffortInputs] = useState<Record<number, string>>({});
  const [source, setSource] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);

  // 搜索与模型列表查看
  const [searchTerm, setSearchTerm] = useState("");

  useEffect(() => {
    if (!open) {
      setConfirmDelete(false);
      setValidationError(null);
      setSearchTerm("");
      setExpandedModels({});
      setEffortInputs({});
      return;
    }
    if (template) {
      setId(template.id);
      setName(template.name);
      setBaseUrl(template.base_url);
      setProtocol(template.protocol);
      setModelsUrl(template.models_url || "");
      setDescription(template.description);
      const nextModels = template.models
        ? template.models.map((model) => ({
            ...model,
            local_model: model.local_model ?? model.upstream_model,
            enabled: model.enabled !== false,
          }))
        : [];
      setModels(nextModels);
      const seeded: GatewayPriceDraft[] = [];
      const seenModels = new Set<string>();
      for (const m of nextModels) {
        const upstream = m.upstream_model.trim();
        if (upstream === "" || seenModels.has(upstream)) continue;
        seenModels.add(upstream);
        const priceRow: ModelPrice = {
          upstream_model: upstream,
          input: m.input ?? 0,
          cache_read: m.cache_read ?? 0,
          cache_write: m.cache_write ?? 0,
          output: m.output ?? 0,
          off_peaks: m.off_peaks ?? (m.off_peak ? [m.off_peak] : []),
        };
        seeded.push(priceRowToDraft(priceRow, upstream, `price-${upstream}`));
      }
      setPriceDrafts(seeded);
      setSource(template.source || "");
    } else {
      setId("");
      setName("");
      setBaseUrl("");
      setProtocol("chat_completions");
      setModelsUrl("");
      setDescription("");
      setModels([]);
      setPriceDrafts([]);
      setSource("");
    }
    setConfirmDelete(false);
    setValidationError(null);
    setSubmitting(false);
    setSearchTerm("");
    setExpandedModels({});
    setEffortInputs({});
  }, [open, template]);

  const usingProviders = useMemo(() => {
    if (!template?.id) return [];
    return providers.filter((p) => p.template_id === template.id);
  }, [template?.id, providers]);

  const isUsed = usingProviders.length > 0;
  const disabled = busy || submitting;

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

  const filteredModels = useMemo(() => {
    const query = searchTerm.trim().toLowerCase();
    if (!query) {
      return models.map((m, index) => ({ model: m, originalIndex: index }));
    }
    return models
      .map((m, index) => ({ model: m, originalIndex: index }))
      .filter(
        ({ model }) =>
          model.upstream_model.toLowerCase().includes(query) ||
          (model.local_model && model.local_model.toLowerCase().includes(query)) ||
          (model.display_name && model.display_name.toLowerCase().includes(query)),
      );
  }, [models, searchTerm]);

  const handleAddModel = () => {
    setModels((prev) => [
      ...prev,
      {
        upstream_model: "",
        local_model: "",
        display_name: "",
        protocol: null,
        enabled: true,
      },
    ]);
  };

  const handleUpdateModel = (
    index: number,
    field: keyof GatewayProviderTemplateModel,
    value: any,
  ) => {
    setModels((prev) => {
      const next = [...prev];
      next[index] = { ...next[index], [field]: value };
      return next;
    });
  };

  const handleRemoveModel = (index: number) => {
    setModels((prev) => prev.filter((_, i) => i !== index));
  };

  const addEffort = (index: number) => {
    const raw = effortInputs[index] ?? "";
    setModels((prev) =>
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
    setModels((prev) =>
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

  const handleSubmit = async () => {
    if (disabled) return;
    const trimmedName = name.trim();
    if (!trimmedName) {
      setValidationError(t("apiGatewayTemplateNameLabel", "Name is required"));
      return;
    }
    const trimmedUrl = baseUrl.trim();
    if (!trimmedUrl) {
      setValidationError(
        t("apiGatewayTemplateBaseUrlLabel", "API base URL is required"),
      );
      return;
    }

    const savedModels: GatewayProviderTemplateModel[] = models
      .filter((m) => m.upstream_model.trim().length > 0)
      .map((m) => {
        const upstream = m.upstream_model.trim();
        const draft = priceDrafts.find((d) => d.upstream_model === upstream);
        const priceRow = draft ? draftToPriceRow(draft) : null;
        const reasoningEfforts = normalizeReasoningEfforts(m.reasoning_efforts);
        return {
          upstream_model: upstream,
          local_model: m.local_model?.trim()
            ? m.local_model.trim()
            : upstream,
          display_name: m.display_name?.trim() ? m.display_name.trim() : undefined,
          protocol: m.protocol ? m.protocol : undefined,
          enabled: m.enabled !== false,
          input: priceRow?.input ?? m.input ?? 0,
          cache_read: priceRow?.cache_read ?? m.cache_read ?? 0,
          cache_write: priceRow?.cache_write ?? m.cache_write ?? 0,
          output: priceRow?.output ?? m.output ?? 0,
          off_peaks: priceRow?.off_peaks ?? m.off_peaks ?? [],
          reasoning_efforts: reasoningEfforts.length > 0 ? reasoningEfforts : undefined,
        };
      });

    const payload: GatewayProviderTemplate = {
      id: id.trim() || `tpl-${Date.now()}`,
      name: trimmedName,
      description: description.trim(),
      base_url: trimmedUrl,
      protocol,
      source: source.trim(),
      models_url: modelsUrl.trim() ? modelsUrl.trim() : null,
      models: savedModels,
    };

    setSubmitting(true);
    try {
      const ok = await onSave(payload);
      if (ok) {
        onOpenChange(false);
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handleDelete = async () => {
    if (!isEditing || isUsed || disabled || !template) return;
    if (!confirmDelete) {
      setConfirmDelete(true);
      return;
    }
    setSubmitting(true);
    try {
      const ok = await onDelete(template.id);
      if (ok) {
        onOpenChange(false);
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="w-full sm:max-w-6xl max-h-[90vh] overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0"
        data-testid="api-gateway-template-edit-dialog"
      >
        <DialogHeader className="pl-6 pr-14 py-4 border-b bg-card/80 backdrop-blur-sm shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-2xs">
              <Sparkles className="h-4.5 w-4.5" />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <DialogTitle className="truncate text-base font-semibold leading-5 text-foreground">
                  {isEditing
                    ? t("apiGatewayEditTemplate", "Edit template")
                    : t("apiGatewayNewTemplate", "New template")}
                </DialogTitle>
                {models.length > 0 ? (
                  <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                    {t("apiGatewayTemplateModelsCount", {
                      count: models.length,
                      defaultValue: `${models.length} 个模型`,
                    })}
                  </span>
                ) : null}
              </div>
              <DialogDescription className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                {t(
                  "apiGatewayEditTemplateDesc",
                  "Configure the template name, API base URL, protocol, description, and models.",
                )}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4.5 p-6">
          {validationError && (
            <div className="flex items-center gap-2 rounded-xl border border-destructive/30 bg-destructive/10 px-3.5 py-2.5 text-xs text-destructive">
              <AlertCircle className="h-4 w-4 shrink-0" />
              <span>{validationError}</span>
            </div>
          )}

          {/* 基础配置两列网格（使用与上游服务商完全一致的标准 field-grid 和 field） */}
          <div className="field-grid col-2 mb-0">
            {/* 第 1 行：名称独占一行 */}
            <div className="field full-span">
              <label className="required">
                {t("apiGatewayTemplateNameLabel", "Name")}
              </label>
              <input
                type="text"
                data-testid="template-edit-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                disabled={disabled}
                placeholder="e.g. OpenCode Zen / DeepSeek"
                aria-label={t("apiGatewayTemplateNameLabel", "Name")}
              />
            </div>

            {/* 第 2 行：接口协议与获取模型列表 URL 并排 */}
            <div className="field">
              <label className="required">
                {t("apiGatewayTemplateProtocolLabel", "Protocol")}
              </label>
              <select
                data-testid="template-edit-protocol"
                value={protocol}
                onChange={(e) =>
                  setProtocol(e.target.value as GatewayUpstreamProtocol)
                }
                disabled={disabled}
                aria-label={t("apiGatewayTemplateProtocolLabel", "Protocol")}
              >
                <option value="chat_completions">
                  {t(
                    "apiGatewayProtocolChat",
                    "Chat Completions (/chat/completions)",
                  )}
                </option>
                <option value="responses">
                  {t("apiGatewayProtocolResponses", "Responses (/responses)")}
                </option>
              </select>
            </div>

            <div className="field">
              <label>{t("apiGatewayTemplateModelsUrl", "Models URL")}</label>
              <input
                type="text"
                data-testid="template-edit-models-url"
                value={modelsUrl}
                onChange={(e) => setModelsUrl(e.target.value)}
                disabled={disabled}
                placeholder={t(
                  "apiGatewayTemplateModelsUrlPlaceholder",
                  "e.g. https://api.openai.com/v1/models (optional)",
                )}
                className="font-mono"
                aria-label={t("apiGatewayTemplateModelsUrl", "Models URL")}
              />
            </div>

            {/* 第 3 行：API Base URL 独占一行 */}
            <div className="field full-span">
              <label className="required">
                {t("apiGatewayTemplateBaseUrlLabel", "API base URL")}
              </label>
              <input
                type="text"
                data-testid="template-edit-base-url"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.target.value)}
                disabled={disabled}
                placeholder="https://api.example.com/v1"
                className="font-mono"
                aria-label={t("apiGatewayTemplateBaseUrlLabel", "API base URL")}
              />
            </div>

            {/* 第 4 行：描述独占一行 */}
            <div className="field full-span">
              <label>{t("description", "Description")}</label>
              <textarea
                data-testid="template-edit-description"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                disabled={disabled}
                rows={2}
                placeholder={t("description", "Description")}
                aria-label={t("description", "Description")}
              />
            </div>
          </div>

          {/* 模型映射维护与查看区域 */}
          <div className="space-y-3 rounded-xl border bg-muted/20 p-3.5">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-xs font-semibold text-foreground">
                    {t("apiGatewayModelMappings", "Model mappings")}
                  </span>
                  <span
                    data-testid="template-edit-models-count"
                    className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary"
                  >
                    {models.length}
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
                <button
                  type="button"
                  data-testid="template-edit-add-model"
                  onClick={handleAddModel}
                  disabled={disabled}
                  className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2 text-xs font-medium shadow-2xs transition hover:bg-muted"
                >
                  <Plus className="h-3 w-3" />
                  {t("apiGatewayAddMapping", "Add mapping")}
                </button>
              </div>
            </div>

            {/* 搜索与过滤栏 */}
            {models.length > 0 && (
              <div className="relative">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <input
                  type="text"
                  data-testid="template-edit-models-search"
                  value={searchTerm}
                  onChange={(e) => setSearchTerm(e.target.value)}
                  placeholder={t(
                    "apiGatewayTemplateSearchModels",
                    "Search models...",
                  )}
                  className={`${mappingInputClass} w-full pl-9 pr-8`}
                />
                {searchTerm && (
                  <button
                    type="button"
                    onClick={() => setSearchTerm("")}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground p-0.5 rounded transition"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                )}
              </div>
            )}

            {/* 模型列表 */}
            {models.length === 0 ? (
              <p className="rounded-lg border border-dashed bg-background/50 px-3 py-4 text-center text-xs text-muted-foreground">
                {t("apiGatewayNoMappings", "No model mappings configured.")}
              </p>
            ) : filteredModels.length === 0 ? (
              <p className="rounded-lg border border-dashed bg-background/50 px-3 py-3 text-center text-xs text-muted-foreground">
                {t("noMatchesFound", "No models matching search")}
              </p>
            ) : (
              <div
                data-testid="template-edit-models-list"
                className="overflow-x-auto"
              >
                <ul className="space-y-2">
                  {filteredModels.map(({ model: m, originalIndex: idx }) => {
                    const isExpanded = expandedModels[idx] === true;
                    const efforts = m.reasoning_efforts ?? [];
                    const upstreamModel = m.upstream_model.trim();
                    return (
                      <li
                        key={idx}
                        data-testid={`template-edit-model-row-${idx}`}
                        data-disabled={m.enabled === false ? "true" : undefined}
                        className={`space-y-2 ${m.enabled === false ? "opacity-60" : ""}`}
                      >
                        <div className="flex items-center gap-2">
                          <button
                            type="button"
                            data-testid={`template-edit-model-expand-${idx}`}
                            aria-expanded={isExpanded}
                            aria-label={t("apiGatewayMappingDetails", "Mapping details")}
                            onClick={() =>
                              setExpandedModels((prev) => ({
                                ...prev,
                                [idx]: !prev[idx],
                              }))
                            }
                            disabled={disabled}
                            className="inline-flex h-[38px] w-[30px] shrink-0 items-center justify-center rounded-lg text-muted-foreground transition hover:bg-muted hover:text-foreground"
                          >
                            {isExpanded ? (
                              <ChevronUp className="h-4 w-4" />
                            ) : (
                              <ChevronDown className="h-4 w-4" />
                            )}
                          </button>
                          <Switch
                            data-testid={`template-edit-model-enabled-${idx}`}
                            aria-label={t("apiGatewayToggleMappingAria", {
                              index: idx + 1,
                              defaultValue: `Enable mapping ${idx + 1}`,
                            })}
                            checked={m.enabled !== false}
                            disabled={disabled}
                            onCheckedChange={(checked) =>
                              handleUpdateModel(idx, "enabled", checked)
                            }
                          />
                          <input
                            type="text"
                            data-testid={`template-edit-model-local-${idx}`}
                            value={m.local_model ?? ""}
                            onChange={(event) =>
                              handleUpdateModel(idx, "local_model", event.target.value)
                            }
                            placeholder={t("apiGatewayLocalModelPlaceholder", "local model")}
                            aria-label={t("apiGatewayLocalModelAria", {
                              index: idx + 1,
                              defaultValue: `Local model ${idx + 1}`,
                            })}
                            disabled={disabled}
                            className={`${mappingInputClass} min-w-[120px] flex-1 font-mono`}
                          />
                          <span aria-hidden="true" className="shrink-0 text-muted-foreground font-semibold text-sm">
                            →
                          </span>
                          <input
                            type="text"
                            data-testid={`template-edit-model-upstream-${idx}`}
                            value={m.upstream_model}
                            onChange={(event) =>
                              handleUpdateModel(idx, "upstream_model", event.target.value)
                            }
                            placeholder={t("apiGatewayUpstreamModelPlaceholder", "upstream model")}
                            aria-label={t("apiGatewayUpstreamModelAria", {
                              index: idx + 1,
                              defaultValue: `Upstream model ${idx + 1}`,
                            })}
                            disabled={disabled}
                            className={`${mappingInputClass} min-w-[120px] flex-1 font-mono`}
                          />
                          <input
                            type="text"
                            data-testid={`template-edit-model-display-${idx}`}
                            value={m.display_name ?? ""}
                            onChange={(event) =>
                              handleUpdateModel(idx, "display_name", event.target.value)
                            }
                            placeholder={t("apiGatewayLocalModelNamePlaceholder", "display name")}
                            aria-label={t("apiGatewayLocalModelNameAria", {
                              index: idx + 1,
                              defaultValue: `Local model name ${idx + 1}`,
                            })}
                            disabled={disabled}
                            className={`${mappingInputClass} min-w-[120px] flex-1`}
                          />
                          <select
                            data-testid={`template-edit-model-protocol-${idx}`}
                            value={m.protocol ?? ""}
                            onChange={(event) =>
                              handleUpdateModel(
                                idx,
                                "protocol",
                                event.target.value === ""
                                  ? null
                                  : (event.target.value as GatewayUpstreamProtocol),
                              )
                            }
                            disabled={disabled}
                            aria-label={t("apiGatewayMappingProtocolAria", {
                              index: idx + 1,
                              defaultValue: `Mapping protocol ${idx + 1}`,
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
                            data-testid={`template-edit-remove-model-${idx}`}
                            onClick={() => handleRemoveModel(idx)}
                            disabled={disabled}
                            aria-label={t("apiGatewayRemoveMappingAria", {
                              index: idx + 1,
                              defaultValue: `Remove mapping ${idx + 1}`,
                            })}
                            className="inline-flex h-[38px] w-[38px] shrink-0 items-center justify-center rounded-lg text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                          >
                            <Trash2 className="h-4 w-4" />
                          </button>
                        </div>

                        {isExpanded ? (
                          <>
                            {upstreamModel !== "" ? (
                              <MappingPriceEditor
                                index={idx}
                                draft={draftForModel(upstreamModel)}
                                onChange={(patch) =>
                                  updateDraftForModel(upstreamModel, patch)
                                }
                              />
                            ) : null}
                            <div
                              data-testid={`api-gateway-mapping-efforts-${idx}`}
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
                                      data-testid={`api-gateway-mapping-effort-${idx}-${effort}`}
                                      className="inline-flex items-center gap-1 rounded-full border border-border bg-secondary px-2 py-0.5 text-[11px] font-medium text-secondary-foreground"
                                    >
                                      <span>{effort}</span>
                                      <button
                                        type="button"
                                        onClick={() => removeEffort(idx, effort)}
                                        aria-label={t("apiGatewayRemoveEffortAria", {
                                          effort,
                                          defaultValue: `Remove reasoning effort ${effort}`,
                                        })}
                                        className="rounded-full p-0.5 hover:bg-muted"
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
                                  data-testid={`api-gateway-mapping-effort-input-${idx}`}
                                  value={effortInputs[idx] ?? ""}
                                  onChange={(event) =>
                                    setEffortInputs((prev) => ({
                                      ...prev,
                                      [idx]: event.target.value,
                                    }))
                                  }
                                  onKeyDown={(event) => {
                                    if (event.key === "Enter") {
                                      event.preventDefault();
                                      addEffort(idx);
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
                                  data-testid={`api-gateway-mapping-effort-add-${idx}`}
                                  onClick={() => addEffort(idx)}
                                  aria-label={t("apiGatewayAddReasoningEffort", "Add")}
                                  className="inline-flex h-8 items-center gap-1 rounded-lg border border-border bg-background px-2.5 text-xs font-medium text-foreground transition hover:bg-muted"
                                >
                                  <Plus className="h-3 w-3" />
                                  {t("apiGatewayAddReasoningEffort", "Add")}
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
          </div>
        </div>

        <DialogFooter className="border-t px-6 py-4 bg-card/80 backdrop-blur-sm shrink-0 flex items-center justify-between gap-3 sm:justify-between">
          <div>
            {isEditing && (
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  data-testid="template-edit-delete-btn"
                  onClick={() => void handleDelete()}
                  disabled={disabled || isUsed}
                  className={`inline-flex h-8 items-center gap-1.5 rounded-lg border px-3 text-xs font-medium transition ${
                    isUsed
                      ? "border-border/60 text-muted-foreground/50 opacity-60 cursor-not-allowed"
                      : confirmDelete
                        ? "border-destructive bg-destructive text-destructive-foreground hover:bg-destructive/90"
                        : "border-destructive/40 text-destructive hover:bg-destructive/10"
                  }`}
                  title={
                    isUsed
                      ? t("apiGatewayTemplateInUseBy", {
                          name: usingProviders.map((p) => p.name).join(", "),
                          defaultValue: `Used by upstream provider "${usingProviders[0]?.name}", cannot be deleted.`,
                        })
                      : undefined
                  }
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {confirmDelete
                    ? t("confirm", "Confirm delete")
                    : t("apiGatewayDeleteTemplate", "Delete template")}
                </button>
                {isUsed && (
                  <span
                    data-testid="template-edit-in-use-warning"
                    className="text-xs text-amber-600 dark:text-amber-400 flex items-center gap-1 max-w-[240px] truncate"
                    title={t("apiGatewayTemplateInUseBy", {
                      name: usingProviders.map((p) => p.name).join(", "),
                    })}
                  >
                    <AlertCircle className="h-3.5 w-3.5 shrink-0" />
                    {t("apiGatewayTemplateInUseBy", {
                      name: usingProviders[0]?.name,
                    })}
                  </span>
                )}
              </div>
            )}
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              disabled={disabled}
              className="inline-flex h-8 items-center rounded-lg border border-border/70 px-3 text-xs font-medium hover:bg-muted active:scale-98 transition"
            >
              {t("cancel", "Cancel")}
            </button>
            <button
              type="button"
              data-testid="template-edit-save-btn"
              onClick={() => void handleSubmit()}
              disabled={disabled}
              className="inline-flex h-8 items-center rounded-lg bg-primary px-3.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 active:scale-98 disabled:opacity-50 shadow-xs transition"
            >
              {t("save", "Save")}
            </button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
