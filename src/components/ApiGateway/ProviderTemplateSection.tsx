import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Check,
  ChevronDown,
  ChevronsUpDown,
  ChevronUp,
  Copy,
  Loader2,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  Sparkles,
  X,
} from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import {
  formatGatewayTimestamp,
  type CreateProviderFromTemplateRequest,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateModel,
  type GatewayProviderTemplateView,
} from "@/lib/apiGateway";
import { TemplateCreateDialog } from "./TemplateCreateDialog";
import { ProviderTemplateAvatar } from "./ProviderTemplateIcon";
import { useTemplateAutoRefreshFailures } from "./useTemplateAutoRefresh";

export type ProviderTemplateSectionProps = {
  templates: GatewayProviderTemplateView[];
  busy: boolean;
  syncingTemplateIds: Record<string, boolean>;
  onSync: (templateId: string) => void;
  onCreateProvider: (request: CreateProviderFromTemplateRequest) => Promise<boolean>;
  onEditTemplate?: (template: GatewayProviderTemplate) => void;
  onNewTemplate?: () => void;
  onResetBuiltin?: () => void;
  hideTitle?: boolean;
  hideHeader?: boolean;
  expandedIds?: Record<string, boolean>;
  onToggleExpand?: (templateId: string) => void;
};

function protocolBadgeClass(protocol: GatewayProviderTemplateModel["protocol"]): string {
  return protocol === "responses"
    ? "bg-purple-500/10 text-purple-600 dark:text-purple-400"
    : "bg-blue-500/10 text-blue-600 dark:text-blue-400";
}

type ProviderTemplateCardProps = {
  view: GatewayProviderTemplateView;
  expanded: boolean;
  syncing: boolean;
  busy: boolean;
  failureReason?: string;
  onToggleExpand: () => void;
  onSync: (templateId: string) => void;
  onCreateProvider: () => void;
  onEditTemplate?: (template: GatewayProviderTemplate) => void;
};

function ProviderTemplateCard({
  view,
  expanded,
  syncing,
  busy,
  failureReason,
  onToggleExpand,
  onSync,
  onCreateProvider,
  onEditTemplate,
}: ProviderTemplateCardProps) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const { template } = view;

  const [searchQuery, setSearchQuery] = useState("");
  const [protocolFilter, setProtocolFilter] = useState<
    "all" | "chat_completions" | "responses"
  >("all");
  const [copiedModel, setCopiedModel] = useState<string | null>(null);

  const protocolLabel =
    template.protocol === "responses" ? "Responses" : "Chat";
  const syncedText = view.synced_at
    ? formatGatewayTimestamp(view.synced_at)
    : t("apiGatewayTemplateNotSynced", "Not synced yet");

  const chatCount = useMemo(
    () =>
      template.models.filter(
        (m) => (m.protocol ?? template.protocol ?? "chat_completions") !== "responses",
      ).length,
    [template.models, template.protocol],
  );

  const responsesCount = useMemo(
    () =>
      template.models.filter(
        (m) => (m.protocol ?? template.protocol ?? "chat_completions") === "responses",
      ).length,
    [template.models, template.protocol],
  );

  const filteredModels = useMemo(() => {
    return template.models.filter((model) => {
      const effectiveProtocol =
        model.protocol ?? template.protocol ?? "chat_completions";
      if (protocolFilter === "responses" && effectiveProtocol !== "responses") {
        return false;
      }
      if (
        protocolFilter === "chat_completions" &&
        effectiveProtocol === "responses"
      ) {
        return false;
      }
      if (searchQuery.trim()) {
        const query = searchQuery.trim().toLowerCase();
        const matchesUpstream = model.upstream_model.toLowerCase().includes(query);
        const matchesDisplay = Boolean(
          model.display_name?.toLowerCase().includes(query),
        );
        if (!matchesUpstream && !matchesDisplay) return false;
      }
      return true;
    });
  }, [template.models, template.protocol, protocolFilter, searchQuery]);

  const handleCopyModel = async (modelName: string) => {
    try {
      await navigator.clipboard.writeText(modelName);
      setCopiedModel(modelName);
      pushToast({
        title: t("apiGatewayTemplateModelCopied", "Model identifier copied"),
        description: modelName,
        kind: "success",
      });
      setTimeout(() => {
        setCopiedModel((curr) => (curr === modelName ? null : curr));
      }, 1500);
    } catch {
      // ignore
    }
  };

  return (
    <div
      data-testid={`api-gateway-template-${template.id}`}
      className="flex flex-col overflow-hidden rounded-2xl border border-border/80 bg-card shadow-xs transition-all duration-200 hover:border-primary/40 hover:shadow-md"
    >
      {/* 头部：品牌图标 + 标题 + 协议标签；右侧次级工具（编辑、同步） */}
      <div className="flex items-center justify-between gap-3 border-b border-border/40 bg-muted/20 px-4.5 py-3.5 sm:px-5">
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <ProviderTemplateAvatar
            icon={template.icon}
            templateId={template.id}
            templateName={template.name}
            size={36}
          />
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <h4
              className="truncate text-sm font-semibold text-foreground leading-5 tracking-tight"
              title={template.name}
            >
              {template.name}
            </h4>
            <span
              className={`inline-flex shrink-0 items-center rounded-md px-1.5 py-0.5 text-[11px] font-medium leading-4 ${protocolBadgeClass(
                template.protocol,
              )}`}
            >
              {protocolLabel}
            </span>
          </div>
        </div>

        {/* 右侧次级操作按钮 */}
        <div className="flex shrink-0 items-center gap-1.5">
          {onEditTemplate && (
            <button
              type="button"
              data-testid={`api-gateway-template-edit-${template.id}`}
              onClick={() => onEditTemplate(template)}
              disabled={busy}
              aria-label={t("apiGatewayEditTemplate", "Edit template")}
              title={t("apiGatewayEditTemplate", "Edit template")}
              className="inline-flex h-7 items-center gap-1 rounded-md border bg-background px-2.5 text-xs font-medium shadow-sm transition hover:bg-muted disabled:opacity-50"
            >
              <Pencil className="h-3 w-3" />
              <span>{t("edit", "Edit")}</span>
            </button>
          )}
          {template.models_url?.trim() ? (
            <button
              type="button"
              data-testid={`api-gateway-template-sync-${template.id}`}
              onClick={() => onSync(template.id)}
              disabled={syncing}
              aria-label={
                syncing
                  ? t("apiGatewayTemplateSyncing", "Syncing models...")
                  : t("apiGatewayTemplateSync", "Sync models")
              }
              title={t("apiGatewayTemplateSync", "Sync models")}
              className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2.5 text-xs font-medium shadow-sm transition hover:bg-muted disabled:opacity-60"
            >
              {syncing ? (
                <Loader2 className="h-3 w-3 animate-spin text-primary" />
              ) : (
                <RefreshCw className="h-3 w-3" />
              )}
              <span>
                {syncing
                  ? t("apiGatewayTemplateSyncing", "Syncing models...")
                  : t("apiGatewayTemplateSync", "Sync models")}
              </span>
            </button>
          ) : null}
        </div>
      </div>

      {/* 主体：描述文案与信息胶囊 */}
      <div className="flex-1 px-4.5 py-4 sm:px-5 space-y-3.5">
        {template.description ? (
          <p className="text-xs leading-relaxed text-muted-foreground line-clamp-2">
            {template.description}
          </p>
        ) : (
          <p className="text-xs italic text-muted-foreground/60">
            {t("apiGatewayNoDescription", "No description provided")}
          </p>
        )}

        {/* 元数据微胶囊 (Chips) 栏 */}
        <div className="flex flex-wrap items-center gap-2 pt-0.5 text-xs">
          <div className="inline-flex items-center gap-1.5 rounded-md border border-border/60 bg-muted/40 px-2.5 py-1 text-[11px] font-medium text-foreground">
            <span>
              {t("apiGatewayTemplateModelsCount", {
                count: template.models.length,
                defaultValue: "{{count}} models",
              })}
            </span>
          </div>

          {template.source && (
            <div
              className="inline-flex max-w-[260px] items-center gap-1.5 rounded-md border border-border/60 bg-muted/40 px-2.5 py-1 text-[11px] text-muted-foreground truncate"
              title={template.source}
            >
              <span className="text-muted-foreground/80 font-medium shrink-0">
                {t("apiGatewayTemplateSource", "Source")}:
              </span>
              <span className="truncate font-mono">{template.source}</span>
            </div>
          )}

          <div className="inline-flex items-center gap-1.5 rounded-md border border-border/60 bg-muted/40 px-2.5 py-1 text-[11px] text-muted-foreground">
            <span className="shrink-0">{t("apiGatewayTemplateLastSync", "Last sync")}:</span>
            <span
              data-testid={`api-gateway-template-synced-${template.id}`}
              className="font-medium text-foreground/80 truncate"
            >
              {syncedText}
            </span>
          </div>
        </div>

        {failureReason ? (
          <div
            data-testid={`api-gateway-template-auto-refresh-failure-${template.id}`}
            className="rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-400"
          >
            {t("apiGatewayTemplateAutoRefreshFailed", { reason: failureReason })}
          </div>
        ) : null}
      </div>

      {/* 底部操作栏：左侧展开控制，右侧添加服务商主按钮 */}
      <div className="flex items-center justify-between gap-3 border-t border-border/50 bg-muted/20 px-4.5 py-2.5 sm:px-5">
        <button
          type="button"
          data-testid={`api-gateway-template-expand-${template.id}`}
          aria-expanded={expanded}
          onClick={onToggleExpand}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-xs font-medium text-muted-foreground transition hover:bg-muted hover:text-foreground active:scale-98"
        >
          {expanded ? (
            <ChevronUp className="h-3.5 w-3.5 text-primary" />
          ) : (
            <ChevronDown className="h-3.5 w-3.5" />
          )}
          <span>
            {expanded
              ? t("apiGatewayTemplateCollapse", "Hide template models")
              : t("apiGatewayTemplateExpand", "Show template models")}
          </span>
          <span className="ml-0.5 rounded-full bg-muted-foreground/10 px-1.5 py-0.2 text-[10px] font-semibold text-muted-foreground">
            {template.models.length}
          </span>
        </button>

        <button
          type="button"
          data-testid={`api-gateway-template-add-${template.id}`}
          onClick={onCreateProvider}
          disabled={busy}
          aria-label={t(
            "apiGatewayTemplateAddProvider",
            "Add as upstream provider",
          )}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
        >
          <Plus className="h-3.5 w-3.5" />
          <span>{t("apiGatewayTemplateAddProvider", "Add as upstream provider")}</span>
        </button>
      </div>

      {/* 展开内容：模型过滤工具栏 + 模型列表 */}
      {expanded ? (
        <div className="border-t border-border/70 bg-muted/15 p-4 sm:p-5 space-y-3">
          {template.models.length === 0 ? (
            <p
              data-testid={`api-gateway-template-no-models-${template.id}`}
              className="rounded-xl border border-dashed bg-card/50 p-6 text-center text-xs text-muted-foreground"
            >
              {t("apiGatewayTemplateNoModels", "No models in this template")}
            </p>
          ) : (
            <>
              {/* 过滤工具栏：搜索框与协议筛选 */}
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2.5">
                <div className="relative flex-1 min-w-[200px]">
                  <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
                  <input
                    type="text"
                    data-testid={`template-model-search-${template.id}`}
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    placeholder={t(
                      "apiGatewayTemplateFilterPlaceholder",
                      "Filter models by name...",
                    )}
                    className="h-8.5 w-full rounded-lg border border-border/80 bg-background pl-9 pr-8 text-xs text-foreground placeholder:text-muted-foreground/60 focus:border-primary focus:outline-none focus:ring-2 focus:ring-primary/20 transition"
                  />
                  {searchQuery ? (
                    <button
                      type="button"
                      onClick={() => setSearchQuery("")}
                      className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition"
                    >
                      <X className="h-3 w-3" />
                    </button>
                  ) : null}
                </div>

                <div
                  className="flex items-center gap-1 rounded-lg border border-border/70 bg-background/80 p-0.5 text-xs shadow-2xs shrink-0"
                  data-testid={`template-model-protocol-filter-${template.id}`}
                >
                  <button
                    type="button"
                    onClick={() => setProtocolFilter("all")}
                    className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
                      protocolFilter === "all"
                        ? "bg-primary text-primary-foreground font-semibold shadow-xs"
                        : "text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    {t("apiGatewayTemplateProtocolAll", "All")} ({template.models.length})
                  </button>
                  {chatCount > 0 && (
                    <button
                      type="button"
                      onClick={() => setProtocolFilter("chat_completions")}
                      className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
                        protocolFilter === "chat_completions"
                          ? "bg-primary text-primary-foreground font-semibold shadow-xs"
                          : "text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      Chat ({chatCount})
                    </button>
                  )}
                  {responsesCount > 0 && (
                    <button
                      type="button"
                      onClick={() => setProtocolFilter("responses")}
                      className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
                        protocolFilter === "responses"
                          ? "bg-primary text-primary-foreground font-semibold shadow-xs"
                          : "text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      Responses ({responsesCount})
                    </button>
                  )}
                </div>
              </div>

              {/* 模型条目容器 */}
              <div className="max-h-80 overflow-y-auto rounded-xl border border-border/70 bg-card divide-y divide-border/40 shadow-xs">
                {filteredModels.length === 0 ? (
                  <div
                    data-testid={`template-no-matching-models-${template.id}`}
                    className="flex flex-col items-center justify-center p-8 text-center text-xs text-muted-foreground space-y-1.5"
                  >
                    <p>
                      {t(
                        "apiGatewayTemplateNoMatchingModels",
                        "No models match the filter",
                      )}
                    </p>
                    <button
                      type="button"
                      onClick={() => {
                        setSearchQuery("");
                        setProtocolFilter("all");
                      }}
                      className="text-xs font-medium text-primary hover:underline"
                    >
                      {t("apiGatewayTemplateClearFilter", "Clear filter")}
                    </button>
                  </div>
                ) : (
                  <>
                    {/* 表头行 */}
                    <div className="sticky top-0 z-10 grid grid-cols-[minmax(0,1.5fr)_minmax(0,1.2fr)_auto] items-center gap-3 border-b border-border/60 bg-muted/70 backdrop-blur-xs px-4 py-2 text-[11px] font-medium text-muted-foreground select-none">
                      <div>{t("upstreamModel", "Upstream Model")}</div>
                      <div>{t("displayName", "Display Name")}</div>
                      <div className="text-right">{t("protocol", "Protocol")}</div>
                    </div>

                    {filteredModels.map((model) => {
                      const isCopied = copiedModel === model.upstream_model;
                      const isDisabled = model.enabled === false;

                      return (
                        <div
                          key={model.upstream_model}
                          data-testid={`api-gateway-template-model-${template.id}-${model.upstream_model}`}
                          data-disabled={isDisabled ? "true" : undefined}
                          className={`grid grid-cols-[minmax(0,1.5fr)_minmax(0,1.2fr)_auto] items-center gap-3 px-4 py-2 text-xs transition hover:bg-muted/40${
                            isDisabled ? " opacity-60 bg-muted/10" : ""
                          }`}
                        >
                          {/* 第 1 列：模型标识与复制按钮 */}
                          <div className="flex min-w-0 items-center gap-1.5">
                            <span
                              className="font-mono text-xs font-semibold text-foreground tracking-tight truncate select-all"
                              title={model.upstream_model}
                            >
                              {model.upstream_model}
                            </span>
                            <button
                              type="button"
                              data-testid={`template-copy-model-${template.id}-${model.upstream_model}`}
                              onClick={() => void handleCopyModel(model.upstream_model)}
                              title={t(
                                "apiGatewayTemplateCopyModelName",
                                "Copy model identifier",
                              )}
                              className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted-foreground/70 hover:bg-muted hover:text-foreground active:scale-95 transition"
                            >
                              {isCopied ? (
                                <Check className="h-3 w-3 text-emerald-600 dark:text-emerald-400" />
                              ) : (
                                <Copy className="h-3 w-3" />
                              )}
                            </button>
                          </div>

                          {/* 第 2 列：显示名称（垂直严格对齐） */}
                          <div className="min-w-0">
                            {model.display_name ? (
                              <span
                                className="truncate text-xs text-muted-foreground block"
                                title={model.display_name}
                              >
                                {model.display_name}
                              </span>
                            ) : (
                              <span className="text-xs text-muted-foreground/30 font-mono select-none">—</span>
                            )}
                          </div>

                          {/* 第 3 列：状态与协议徽标（统一右对齐） */}
                          <div className="flex shrink-0 items-center justify-end gap-1.5">
                            {isDisabled ? (
                              <span className="inline-flex items-center rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium leading-4 text-muted-foreground">
                                {t(
                                  "apiGatewayTemplateModelDisabled",
                                  "Disabled",
                                )}
                              </span>
                            ) : null}
                            <span
                              className={`inline-flex items-center justify-center min-w-[58px] rounded-full px-2 py-0.5 text-[10px] font-medium leading-4 text-center ${protocolBadgeClass(
                                model.protocol,
                              )}`}
                            >
                              {model.protocol === "responses" ? "Responses" : "Chat"}
                            </span>
                          </div>
                        </div>
                      );
                    })}
                  </>
                )}
              </div>
            </>
          )}
        </div>
      ) : null}
    </div>
  );
}

export function ProviderTemplateSection({
  templates,
  busy,
  syncingTemplateIds,
  onSync,
  onCreateProvider,
  onEditTemplate,
  onNewTemplate,
  onResetBuiltin,
  hideTitle,
  hideHeader,
  expandedIds,
  onToggleExpand,
}: ProviderTemplateSectionProps) {
  const { t } = useTranslation();
  const templateFailures = useTemplateAutoRefreshFailures();
  const [internalExpandedIds, setInternalExpandedIds] = useState<Record<string, boolean>>({});
  const activeExpandedIds = expandedIds ?? internalExpandedIds;
  const [createView, setCreateView] = useState<GatewayProviderTemplateView | null>(
    null,
  );

  const allExpanded = useMemo(() => {
    if (templates.length === 0) return false;
    return templates.every((view) => Boolean(activeExpandedIds[view.template.id]));
  }, [templates, activeExpandedIds]);

  const toggleExpanded = (templateId: string) => {
    if (onToggleExpand) {
      onToggleExpand(templateId);
    } else {
      setInternalExpandedIds((prev) => ({ ...prev, [templateId]: !prev[templateId] }));
    }
  };

  const handleToggleAll = () => {
    if (allExpanded) {
      if (onToggleExpand) {
        for (const view of templates) {
          if (activeExpandedIds[view.template.id]) {
            onToggleExpand(view.template.id);
          }
        }
      } else {
        setInternalExpandedIds({});
      }
    } else {
      if (onToggleExpand) {
        for (const view of templates) {
          if (!activeExpandedIds[view.template.id]) {
            onToggleExpand(view.template.id);
          }
        }
      } else {
        const next: Record<string, boolean> = {};
        for (const view of templates) {
          next[view.template.id] = true;
        }
        setInternalExpandedIds(next);
      }
    }
  };

  return (
    <section className="space-y-4" data-testid="api-gateway-provider-templates">
      {/* 头部标题与全局操作工具栏（外部 Header 接管时隐藏） */}
      {!hideHeader && (
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between pb-1">
          <div className="flex items-center gap-2.5">
            {!hideTitle ? (
              <>
                <div className="flex items-center gap-1.5">
                  <Sparkles className="h-4 w-4 text-primary" />
                  <h3 className="text-sm font-semibold text-foreground">
                    {t("apiGatewayProviderTemplates", "Provider Templates")}
                  </h3>
                </div>
                <span className="rounded-full bg-primary/10 px-2 py-0.5 text-xs font-semibold text-primary">
                  {templates.length}
                </span>
              </>
            ) : (
              <span className="text-xs text-muted-foreground font-medium">
                {t("apiGatewayAvailableTemplatesCount", {
                  count: templates.length,
                  defaultValue: "共 {{count}} 个可用模板",
                })}
              </span>
            )}
          </div>

          <div className="flex flex-wrap items-center gap-2">
            {templates.length > 0 && (
              <button
                type="button"
                data-testid="template-section-toggle-all-btn"
                onClick={handleToggleAll}
                disabled={busy}
                className="inline-flex h-8 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-sm transition hover:bg-muted disabled:opacity-50"
              >
                <ChevronsUpDown className="h-3.5 w-3.5" />
                <span>
                  {allExpanded
                    ? t("apiGatewayTemplateCollapseAll", "Collapse all")
                    : t("apiGatewayTemplateExpandAll", "Expand all")}
                </span>
              </button>
            )}

            {onResetBuiltin && (
              <button
                type="button"
                data-testid="template-section-reset-btn"
                onClick={onResetBuiltin}
                disabled={busy}
                title={t("apiGatewayTemplateResetBuiltin", "Restore built-in presets")}
                className="inline-flex h-8 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-sm transition hover:bg-muted disabled:opacity-50"
              >
                <RotateCcw className="h-3.5 w-3.5" />
                <span>{t("apiGatewayTemplateResetBuiltin", "Restore built-in presets")}</span>
              </button>
            )}

            {onNewTemplate && (
              <button
                type="button"
                data-testid="template-section-new-btn"
                onClick={onNewTemplate}
                disabled={busy}
                className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-xs transition hover:bg-primary/90 active:scale-98 disabled:opacity-50"
              >
                <Plus className="h-3.5 w-3.5" />
                <span>{t("apiGatewayNewTemplate", "New template")}</span>
              </button>
            )}
          </div>
        </div>
      )}

      {/* 模板列表 */}
      {templates.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed bg-card/40 px-6 py-12 text-center">
          <div className="rounded-full bg-muted/80 p-3 text-muted-foreground">
            <Sparkles className="h-6 w-6 opacity-70" />
          </div>
          <h4 className="mt-3 text-xs font-medium text-foreground">
            {t("apiGatewayNoTemplates", "No provider templates available.")}
          </h4>
        </div>
      ) : (
        <div className="grid grid-cols-1 xl:grid-cols-2 gap-5">
          {templates.map((view) => (
            <ProviderTemplateCard
              key={view.template.id}
              view={view}
              expanded={Boolean(activeExpandedIds[view.template.id])}
              syncing={Boolean(syncingTemplateIds[view.template.id])}
              busy={busy}
              failureReason={templateFailures[view.template.id]}
              onToggleExpand={() => toggleExpanded(view.template.id)}
              onSync={onSync}
              onCreateProvider={() => setCreateView(view)}
              onEditTemplate={onEditTemplate}
            />
          ))}
        </div>
      )}

      <TemplateCreateDialog
        open={createView !== null}
        onOpenChange={(open) => {
          if (!open) setCreateView(null);
        }}
        template={createView?.template ?? null}
        busy={busy}
        onConfirm={onCreateProvider}
      />
    </section>
  );
}
