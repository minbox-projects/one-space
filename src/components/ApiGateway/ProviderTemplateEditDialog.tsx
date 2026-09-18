import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertCircle,
  Check,
  Globe,
  Loader2,
  Plus,
  RotateCw,
  Search,
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
import {
  apiGatewayFetchModels,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateModel,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
} from "@/lib/apiGateway";

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
  const [source, setSource] = useState("");
  const [snapshotVersion, setSnapshotVersion] = useState("1");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);

  // 搜索与模型列表查看
  const [searchTerm, setSearchTerm] = useState("");

  // 获取远端模型面板状态
  const [showFetchModal, setShowFetchModal] = useState(false);
  const [fetchApiKey, setFetchApiKey] = useState("");
  const [fetchingModels, setFetchingModels] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [fetchedModels, setFetchedModels] = useState<string[]>([]);
  const [selectedFetchedModels, setSelectedFetchedModels] = useState<Set<string>>(
    new Set(),
  );

  useEffect(() => {
    if (!open) {
      setConfirmDelete(false);
      setValidationError(null);
      setShowFetchModal(false);
      setSearchTerm("");
      return;
    }
    if (template) {
      setId(template.id);
      setName(template.name);
      setBaseUrl(template.base_url);
      setProtocol(template.protocol);
      setModelsUrl(template.models_url || "");
      setDescription(template.description);
      setModels(template.models ? [...template.models] : []);
      setSource(template.source || "");
      setSnapshotVersion(template.snapshot_version || "1");
    } else {
      setId("");
      setName("");
      setBaseUrl("");
      setProtocol("chat_completions");
      setModelsUrl("");
      setDescription("");
      setModels([]);
      setSource("");
      setSnapshotVersion("1");
    }
    setConfirmDelete(false);
    setValidationError(null);
    setSubmitting(false);
    setShowFetchModal(false);
    setSearchTerm("");
    setFetchedModels([]);
    setSelectedFetchedModels(new Set());
    setFetchError(null);
  }, [open, template]);

  const usingProviders = useMemo(() => {
    if (!template?.id) return [];
    return providers.filter((p) => p.template_id === template.id);
  }, [template?.id, providers]);

  const isUsed = usingProviders.length > 0;
  const disabled = busy || submitting;

  const targetFetchUrl = useMemo(() => {
    const explicit = modelsUrl.trim();
    if (explicit) return explicit;
    const base = baseUrl.trim().replace(/\/+$/, "");
    if (!base) return "";
    if (base.endsWith("/models")) return base;
    if (base.endsWith("/v1")) return `${base}/models`;
    return `${base}/v1/models`;
  }, [modelsUrl, baseUrl]);

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
          (model.display_name && model.display_name.toLowerCase().includes(query)),
      );
  }, [models, searchTerm]);

  const handleAddModel = () => {
    setModels((prev) => [
      ...prev,
      {
        upstream_model: "",
        display_name: "",
        input: 0,
        cache_read: 0,
        cache_write: 0,
        output: 0,
        off_peaks: [],
        reasoning_efforts: [],
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

  const handleStartFetchModels = async () => {
    if (!targetFetchUrl) {
      setFetchError(
        t(
          "apiGatewayTemplateBaseUrlLabel",
          "API base URL or Models URL is required",
        ),
      );
      return;
    }
    setFetchingModels(true);
    setFetchError(null);
    try {
      const list = await apiGatewayFetchModels(targetFetchUrl, fetchApiKey);
      setFetchedModels(list);
      const existingIds = new Set(models.map((m) => m.upstream_model.trim()));
      const initialSelected = new Set(list.filter((id) => !existingIds.has(id)));
      setSelectedFetchedModels(initialSelected);
    } catch (err: any) {
      setFetchError(err?.message || String(err));
    } finally {
      setFetchingModels(false);
    }
  };

  const handleToggleSelectAllFetched = () => {
    const existingIds = new Set(models.map((m) => m.upstream_model.trim()));
    const unadded = fetchedModels.filter((id) => !existingIds.has(id));
    if (selectedFetchedModels.size === unadded.length) {
      setSelectedFetchedModels(new Set());
    } else {
      setSelectedFetchedModels(new Set(unadded));
    }
  };

  const handleImportFetched = () => {
    const existingIds = new Set(models.map((m) => m.upstream_model.trim()));
    const newModels: GatewayProviderTemplateModel[] = [];
    for (const id of fetchedModels) {
      if (selectedFetchedModels.has(id) && !existingIds.has(id)) {
        newModels.push({
          upstream_model: id,
          display_name: "",
          input: 0,
          cache_read: 0,
          cache_write: 0,
          output: 0,
          off_peaks: [],
          reasoning_efforts: [],
        });
        existingIds.add(id);
      }
    }
    if (newModels.length > 0) {
      setModels((prev) => [...prev, ...newModels]);
    }
    setShowFetchModal(false);
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

    const payload: GatewayProviderTemplate = {
      id: id.trim() || `tpl-${Date.now()}`,
      name: trimmedName,
      description: description.trim(),
      base_url: trimmedUrl,
      protocol,
      source: source.trim(),
      snapshot_version: snapshotVersion.trim() || "1",
      models_url: modelsUrl.trim() ? modelsUrl.trim() : null,
      models: models.filter((m) => m.upstream_model.trim().length > 0),
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
        className="w-full p-5 sm:max-w-4xl sm:rounded-xl max-h-[90vh] flex flex-col"
        data-testid="api-gateway-template-edit-dialog"
      >
        <DialogHeader className="space-y-1">
          <DialogTitle className="text-base font-semibold">
            {isEditing
              ? t("apiGatewayEditTemplate", "Edit template")
              : t("apiGatewayNewTemplate", "New template")}
          </DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            {t(
              "apiGatewayEditTemplateDesc",
              "Configure the template name, API base URL, protocol, description, and models.",
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 py-2 px-1.5 -mx-1.5">
          {validationError && (
            <div className="flex items-center gap-1.5 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
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

          {/* 模型维护与查看区域 */}
          <div className="space-y-3 rounded-xl border bg-muted/20 p-3.5">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <span className="text-xs font-semibold text-foreground">
                  {t("models", "Models")}
                </span>
                <span
                  data-testid="template-edit-models-count"
                  className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary"
                >
                  {models.length}
                </span>
              </div>

              <div className="flex items-center gap-2">
                <button
                  type="button"
                  data-testid="template-edit-fetch-models-btn"
                  onClick={() => setShowFetchModal((prev) => !prev)}
                  disabled={disabled}
                  className={`inline-flex h-8 items-center gap-1.5 rounded-lg border px-3 text-xs font-medium shadow-sm transition ${
                    showFetchModal
                      ? "bg-primary text-primary-foreground border-primary"
                      : "bg-background hover:bg-muted text-foreground"
                  }`}
                  title={t(
                    "apiGatewayTemplateFetchModels",
                    "Fetch models from endpoint",
                  )}
                >
                  <Globe className="h-3.5 w-3.5" />
                  {t("apiGatewayTemplateFetchModels", "Fetch models")}
                </button>

                <button
                  type="button"
                  data-testid="template-edit-add-model"
                  onClick={handleAddModel}
                  disabled={disabled}
                  className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
                >
                  <Plus className="h-3.5 w-3.5" />
                  {t("add", "Add model")}
                </button>
              </div>
            </div>

            {/* 获取远端模型面板 */}
            {showFetchModal && (
              <div
                data-testid="template-edit-fetch-panel"
                className="space-y-3 rounded-xl border bg-background/80 p-3.5 text-xs shadow-xs"
              >
                <div className="flex items-center justify-between">
                  <div className="font-semibold text-foreground flex items-center gap-1.5 text-sm">
                    <Globe className="h-4 w-4 text-primary" />
                    <span>
                      {t("apiGatewayTemplateFetchModels", "Fetch models")}
                    </span>
                  </div>
                  <button
                    type="button"
                    onClick={() => setShowFetchModal(false)}
                    className="text-muted-foreground hover:text-foreground p-1 rounded-md transition"
                  >
                    <X className="h-4 w-4" />
                  </button>
                </div>

                <div className="space-y-2.5">
                  <div>
                    <div className="text-[11px] text-muted-foreground mb-1.5">
                      {t("targetUrl", "Target URL")}:{" "}
                      <span className="font-mono text-foreground font-semibold">
                        {targetFetchUrl || "(Not configured)"}
                      </span>
                    </div>
                    <div className="flex gap-2">
                      <input
                        type="text"
                        data-testid="template-edit-fetch-api-key"
                        value={fetchApiKey}
                        onChange={(e) => setFetchApiKey(e.target.value)}
                        placeholder={t(
                          "apiGatewayTemplateFetchModelsApiKey",
                          "API Key (optional, leave empty if public)",
                        )}
                        className={`${mappingInputClass} flex-1 font-mono`}
                      />
                      <button
                        type="button"
                        data-testid="template-edit-fetch-start-btn"
                        onClick={() => void handleStartFetchModels()}
                        disabled={fetchingModels || !targetFetchUrl}
                        className="inline-flex h-[38px] items-center gap-1.5 rounded-lg bg-primary px-4 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 shadow-sm transition"
                      >
                        {fetchingModels ? (
                          <>
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                            {t(
                              "apiGatewayTemplateFetchingModels",
                              "Fetching...",
                            )}
                          </>
                        ) : (
                          <>
                            <RotateCw className="h-3.5 w-3.5" />
                            {t("apiGatewayTemplateFetchModels", "Fetch")}
                          </>
                        )}
                      </button>
                    </div>
                  </div>

                  {fetchError && (
                    <div className="rounded-lg border border-destructive/30 bg-destructive/10 p-2.5 text-xs text-destructive">
                      {fetchError}
                    </div>
                  )}

                  {fetchedModels.length > 0 && (
                    <div className="space-y-2 pt-2 border-t">
                      <div className="flex items-center justify-between text-xs">
                        <span className="text-muted-foreground">
                          {t("apiGatewayTemplateFetchModelsSuccess", {
                            count: fetchedModels.length,
                          })}
                        </span>
                        <button
                          type="button"
                          onClick={handleToggleSelectAllFetched}
                          className="text-primary hover:underline font-medium"
                        >
                          {selectedFetchedModels.size > 0
                            ? t("apiGatewayTemplateDeselectAll", "Deselect all")
                            : t("apiGatewayTemplateSelectAll", "Select all")}
                        </button>
                      </div>

                      <div className="max-h-40 overflow-y-auto space-y-1.5 rounded-lg border bg-background p-2">
                        {fetchedModels.map((mId) => {
                          const alreadyAdded = models.some(
                            (m) => m.upstream_model.trim() === mId,
                          );
                          const isSelected = selectedFetchedModels.has(mId);
                          return (
                            <label
                              key={mId}
                              className={`flex items-center gap-2 rounded-md px-2.5 py-1.5 text-xs cursor-pointer select-none transition ${
                                alreadyAdded
                                  ? "opacity-50 cursor-not-allowed bg-muted/40"
                                  : isSelected
                                    ? "bg-primary/10 text-primary font-medium"
                                    : "hover:bg-muted"
                              }`}
                            >
                              <input
                                type="checkbox"
                                disabled={alreadyAdded}
                                checked={alreadyAdded || isSelected}
                                onChange={(e) => {
                                  if (alreadyAdded) return;
                                  const next = new Set(selectedFetchedModels);
                                  if (e.target.checked) {
                                    next.add(mId);
                                  } else {
                                    next.delete(mId);
                                  }
                                  setSelectedFetchedModels(next);
                                }}
                                className="h-4 w-4 rounded border-border text-primary focus:ring-primary"
                              />
                              <span className="font-mono flex-1 truncate">
                                {mId}
                              </span>
                              {alreadyAdded && (
                                <span className="text-[10px] text-muted-foreground italic">
                                  {t("alreadyAdded", "Already added")}
                                </span>
                              )}
                            </label>
                          );
                        })}
                      </div>

                      <div className="flex justify-end pt-1">
                        <button
                          type="button"
                          data-testid="template-edit-import-fetched-btn"
                          onClick={handleImportFetched}
                          disabled={selectedFetchedModels.size === 0}
                          className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 shadow-sm transition"
                        >
                          <Check className="h-3.5 w-3.5" />
                          {t("apiGatewayTemplateImportSelected", {
                            count: selectedFetchedModels.size,
                          })}
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}

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
                {t("apiGatewayTemplateNoModels", "No models in this template")}
              </p>
            ) : filteredModels.length === 0 ? (
              <p className="rounded-lg border border-dashed bg-background/50 px-3 py-3 text-center text-xs text-muted-foreground">
                {t("noMatchesFound", "No models matching search")}
              </p>
            ) : (
              <div
                data-testid="template-edit-models-list"
                className="space-y-2 max-h-64 overflow-y-auto pr-1"
              >
                {filteredModels.map(({ model: m, originalIndex: idx }) => (
                  <div
                    key={idx}
                    className="flex items-center gap-2 rounded-xl border bg-card p-2.5 text-xs shadow-xs"
                  >
                    <div className="flex-1 min-w-0 grid grid-cols-1 sm:grid-cols-2 gap-2">
                      <input
                        type="text"
                        placeholder={t("upstreamModel", "Upstream model")}
                        value={m.upstream_model}
                        onChange={(e) =>
                          handleUpdateModel(
                            idx,
                            "upstream_model",
                            e.target.value,
                          )
                        }
                        disabled={disabled}
                        className={`${mappingInputClass} font-mono w-full`}
                      />
                      <input
                        type="text"
                        placeholder={t(
                          "displayName",
                          "Display name (optional)",
                        )}
                        value={m.display_name ?? ""}
                        onChange={(e) =>
                          handleUpdateModel(idx, "display_name", e.target.value)
                        }
                        disabled={disabled}
                        className={`${mappingInputClass} w-full`}
                      />
                    </div>

                    <div className="flex items-center gap-2">
                      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                        <span>{t("in", "In")}:</span>
                        <input
                          type="number"
                          step="any"
                          min="0"
                          value={m.input}
                          onChange={(e) =>
                            handleUpdateModel(
                              idx,
                              "input",
                              parseFloat(e.target.value) || 0,
                            )
                          }
                          disabled={disabled}
                          title={t(
                            "apiGatewayTemplateInputPrice",
                            "Input ($/1M)",
                          )}
                          className={`${mappingInputClass} w-20 font-mono text-right`}
                        />
                      </div>
                      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                        <span>{t("out", "Out")}:</span>
                        <input
                          type="number"
                          step="any"
                          min="0"
                          value={m.output}
                          onChange={(e) =>
                            handleUpdateModel(
                              idx,
                              "output",
                              parseFloat(e.target.value) || 0,
                            )
                          }
                          disabled={disabled}
                          title={t(
                            "apiGatewayTemplateOutputPrice",
                            "Output ($/1M)",
                          )}
                          className={`${mappingInputClass} w-20 font-mono text-right`}
                        />
                      </div>
                    </div>

                    <button
                      type="button"
                      data-testid={`template-edit-remove-model-${idx}`}
                      onClick={() => handleRemoveModel(idx)}
                      disabled={disabled}
                      className="text-muted-foreground hover:text-destructive p-2 rounded-lg transition shrink-0"
                      title={t("delete", "Delete")}
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        <DialogFooter className="border-t pt-3 flex items-center justify-between gap-2 sm:justify-between">
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
              className="inline-flex h-8 items-center rounded-lg border px-3 text-xs font-medium hover:bg-muted transition"
            >
              {t("cancel", "Cancel")}
            </button>
            <button
              type="button"
              data-testid="template-edit-save-btn"
              onClick={() => void handleSubmit()}
              disabled={disabled}
              className="inline-flex h-8 items-center rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 shadow-sm transition"
            >
              {t("save", "Save")}
            </button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
