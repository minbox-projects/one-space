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

export type ProviderTemplateSectionProps = {
  templates: GatewayProviderTemplateView[];
  busy: boolean;
  syncingTemplateIds: Record<string, boolean>;
  onSync: (templateId: string) => void;
  onCreateProvider: (request: CreateProviderFromTemplateRequest) => Promise<boolean>;
  onEditTemplate?: (template: GatewayProviderTemplate) => void;
  onNewTemplate?: () => void;
  onResetBuiltin?: () => void;
};

/** Offline brand accents: OpenCode Zen emerald/cyan, CommandCode indigo/violet. */
function brandAccent(templateId: string, name: string): string {
  const key = `${templateId} ${name}`.toLowerCase();
  if (key.includes("command")) {
    return "bg-indigo-500/10 text-indigo-600 dark:text-indigo-400";
  }
  return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400";
}

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
      className="flex flex-col overflow-hidden rounded-xl border bg-card shadow-xs transition-all hover:border-primary/40 hover:shadow-md"
    >
      {/* 头部元数据与操作 */}
      <div className="flex items-start gap-3.5 p-4">
        <div
          className={`rounded-lg p-2.5 shrink-0 ${brandAccent(template.id, template.name)}`}
        >
          <Sparkles className="h-4 w-4" />
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <h4 className="truncate text-sm font-semibold text-foreground">
              {template.name}
            </h4>
            <span
              className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[11px] font-medium leading-4 ${protocolBadgeClass(
                template.protocol,
              )}`}
            >
              {protocolLabel}
            </span>
          </div>

          <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
            {template.description}
          </p>

          <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
            <span className="font-medium text-foreground/80">
              {t("apiGatewayTemplateModelsCount", {
                count: template.models.length,
                defaultValue: "{{count}} models",
              })}
            </span>
            <span className="truncate max-w-[220px]" title={template.source}>
              {t("apiGatewayTemplateSource", "Source")}: {template.source}
            </span>
            <span>
              {t("apiGatewayTemplateLastSync", "Last sync")}:{" "}
              <span data-testid={`api-gateway-template-synced-${template.id}`}>
                {syncedText}
              </span>
            </span>
          </div>
        </div>

        {/* 右侧操作按钮组 */}
        <div className="flex shrink-0 flex-col items-end gap-2">
          <div className="flex items-center gap-1.5">
            {onEditTemplate && (
              <button
                type="button"
                data-testid={`api-gateway-template-edit-${template.id}`}
                onClick={() => onEditTemplate(template)}
                disabled={busy}
                aria-label={t("apiGatewayEditTemplate", "Edit template")}
                title={t("apiGatewayEditTemplate", "Edit template")}
                className="inline-flex h-7 items-center gap-1 rounded-md border bg-background px-2 text-[11px] font-medium shadow-xs transition hover:bg-muted disabled:opacity-50"
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
                className="inline-flex h-7 items-center gap-1 rounded-md border bg-background px-2 text-[11px] font-medium shadow-xs transition hover:bg-muted disabled:opacity-60"
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

          <button
            type="button"
            data-testid={`api-gateway-template-add-${template.id}`}
            onClick={onCreateProvider}
            disabled={busy}
            aria-label={t(
              "apiGatewayTemplateAddProvider",
              "Add as upstream provider",
            )}
            className="inline-flex h-7 items-center gap-1.5 rounded-md bg-primary px-2.5 text-[11px] font-medium text-primary-foreground shadow-xs transition hover:bg-primary/90 disabled:opacity-50"
          >
            <Plus className="h-3.5 w-3.5" />
            <span>{t("apiGatewayTemplateAddProvider", "Add as upstream provider")}</span>
          </button>
        </div>
      </div>

      {/* 展开/收起模型栏控制 */}
      <button
        type="button"
        data-testid={`api-gateway-template-expand-${template.id}`}
        aria-expanded={expanded}
        onClick={onToggleExpand}
        className="inline-flex h-8 items-center justify-center gap-1.5 border-t bg-muted/30 text-[11px] font-medium text-muted-foreground transition hover:bg-muted/60 hover:text-foreground"
      >
        {expanded ? (
          <ChevronUp className="h-3.5 w-3.5" />
        ) : (
          <ChevronDown className="h-3.5 w-3.5" />
        )}
        <span>
          {expanded
            ? t("apiGatewayTemplateCollapse", "Hide template models")
            : t("apiGatewayTemplateExpand", "Show template models")}
        </span>
      </button>

      {/* 展开内容：模型过滤工具栏 + 模型列表 */}
      {expanded ? (
        <div className="border-t bg-muted/15">
          {template.models.length === 0 ? (
            <p
              data-testid={`api-gateway-template-no-models-${template.id}`}
              className="p-4 text-center text-xs text-muted-foreground"
            >
              {t("apiGatewayTemplateNoModels", "No models in this template")}
            </p>
          ) : (
            <>
              {/* 过滤工具栏：搜索框与协议筛选 */}
              <div className="flex flex-wrap items-center justify-between gap-2 border-b bg-muted/40 px-3.5 py-2">
                <div className="relative min-w-[160px] flex-1">
                  <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
                  <input
                    type="text"
                    data-testid={`template-model-search-${template.id}`}
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    placeholder={t(
                      "apiGatewayTemplateFilterPlaceholder",
                      "Filter models by name...",
                    )}
                    className="h-7 w-full rounded-md border border-border/70 bg-background pl-8 pr-7 text-xs text-foreground placeholder:text-muted-foreground/60 focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40"
                  />
                  {searchQuery ? (
                    <button
                      type="button"
                      onClick={() => setSearchQuery("")}
                      className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                    >
                      <X className="h-3 w-3" />
                    </button>
                  ) : null}
                </div>

                <div
                  className="flex items-center gap-1 rounded-lg border bg-background/80 p-0.5 text-[11px]"
                  data-testid={`template-model-protocol-filter-${template.id}`}
                >
                  <button
                    type="button"
                    onClick={() => setProtocolFilter("all")}
                    className={`rounded px-2 py-0.5 font-medium transition ${
                      protocolFilter === "all"
                        ? "bg-primary text-primary-foreground font-semibold"
                        : "text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    {t("apiGatewayTemplateProtocolAll", "All")} (
                    {template.models.length})
                  </button>
                  {chatCount > 0 && (
                    <button
                      type="button"
                      onClick={() => setProtocolFilter("chat_completions")}
                      className={`rounded px-2 py-0.5 font-medium transition ${
                        protocolFilter === "chat_completions"
                          ? "bg-primary text-primary-foreground font-semibold"
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
                      className={`rounded px-2 py-0.5 font-medium transition ${
                        protocolFilter === "responses"
                          ? "bg-primary text-primary-foreground font-semibold"
                          : "text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      Responses ({responsesCount})
                    </button>
                  )}
                </div>
              </div>

              {/* 模型条目容器 */}
              <div className="max-h-72 overflow-y-auto divide-y divide-border/50">
                {filteredModels.length === 0 ? (
                  <div
                    data-testid={`template-no-matching-models-${template.id}`}
                    className="flex flex-col items-center justify-center p-6 text-center text-xs text-muted-foreground"
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
                      className="mt-2 text-xs font-medium text-primary hover:underline"
                    >
                      {t("apiGatewayTemplateClearFilter", "Clear filter")}
                    </button>
                  </div>
                ) : (
                  filteredModels.map((model) => {
                    const isCopied = copiedModel === model.upstream_model;
                    const isDisabled = model.enabled === false;

                    return (
                      <div
                        key={model.upstream_model}
                        data-testid={`api-gateway-template-model-${template.id}-${model.upstream_model}`}
                        data-disabled={isDisabled ? "true" : undefined}
                        className={`space-y-2 p-3 transition-colors hover:bg-muted/30${
                          isDisabled ? " opacity-60" : ""
                        }`}
                      >
                        {/* 模型标识、显示名、协议与复制按钮 */}
                        <div className="flex flex-wrap items-center justify-between gap-1.5">
                          <div className="flex flex-wrap items-center gap-1.5">
                            <span className="font-mono text-xs font-semibold text-foreground">
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
                              className="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground transition hover:bg-muted hover:text-foreground"
                            >
                              {isCopied ? (
                                <Check className="h-3 w-3 text-emerald-600 dark:text-emerald-400" />
                              ) : (
                                <Copy className="h-3 w-3" />
                              )}
                            </button>
                            {model.display_name ? (
                              <span className="text-[11px] text-muted-foreground">
                                {model.display_name}
                              </span>
                            ) : null}
                          </div>

                          <div className="flex items-center gap-1.5">
                            {isDisabled ? (
                              <span className="inline-flex items-center rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium leading-4 text-muted-foreground">
                                {t(
                                  "apiGatewayTemplateModelDisabled",
                                  "Disabled",
                                )}
                              </span>
                            ) : null}
                            <span
                              className={`inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium leading-4 ${protocolBadgeClass(
                                model.protocol,
                              )}`}
                            >
                              {model.protocol === "responses" ? "Responses" : "Chat"}
                            </span>
                          </div>
                        </div>
                      </div>
                    );
                  })
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
}: ProviderTemplateSectionProps) {
  const { t } = useTranslation();
  const [expandedIds, setExpandedIds] = useState<Record<string, boolean>>({});
  const [createView, setCreateView] = useState<GatewayProviderTemplateView | null>(
    null,
  );

  const allExpanded = useMemo(() => {
    if (templates.length === 0) return false;
    return templates.every((view) => Boolean(expandedIds[view.template.id]));
  }, [templates, expandedIds]);

  const toggleExpanded = (templateId: string) => {
    setExpandedIds((prev) => ({ ...prev, [templateId]: !prev[templateId] }));
  };

  const handleToggleAll = () => {
    if (allExpanded) {
      setExpandedIds({});
    } else {
      const next: Record<string, boolean> = {};
      for (const view of templates) {
        next[view.template.id] = true;
      }
      setExpandedIds(next);
    }
  };

  return (
    <section className="space-y-3" data-testid="api-gateway-provider-templates">
      {/* 头部标题与操作栏 */}
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2">
            <Sparkles className="h-4 w-4 text-primary" />
            <h3 className="text-sm font-semibold text-foreground">
              {t("apiGatewayProviderTemplates", "Provider Templates")}
            </h3>
            <span className="rounded-full bg-muted px-2 py-0.2 text-[10px] font-semibold text-muted-foreground">
              {templates.length}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            {t(
              "apiGatewayProviderTemplatesDesc",
              "Provider templates carry an optional model-list URL. Sync to refresh the model list, then add one as an upstream provider; prices stay on mapping rows.",
            )}
          </p>
        </div>

        <div className="flex items-center gap-2">
          {templates.length > 0 && (
            <button
              type="button"
              data-testid="template-section-toggle-all-btn"
              onClick={handleToggleAll}
              disabled={busy}
              className="inline-flex h-8 items-center gap-1.5 rounded-lg border bg-background px-2.5 text-xs font-medium shadow-2xs transition hover:bg-muted disabled:opacity-50"
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
              className="inline-flex h-8 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-2xs transition hover:bg-muted disabled:opacity-50"
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
              className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-2xs transition hover:bg-primary/90 disabled:opacity-50"
            >
              <Plus className="h-3.5 w-3.5" />
              <span>{t("apiGatewayNewTemplate", "New template")}</span>
            </button>
          )}
        </div>
      </div>

      {/* 模板列表 */}
      {templates.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/50 px-6 py-8 text-center">
          <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
            <Sparkles className="h-5 w-5 opacity-70" />
          </div>
          <p className="mt-2.5 text-xs text-muted-foreground">
            {t("apiGatewayNoTemplates", "No provider templates available.")}
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {templates.map((view) => (
            <ProviderTemplateCard
              key={view.template.id}
              view={view}
              expanded={Boolean(expandedIds[view.template.id])}
              syncing={Boolean(syncingTemplateIds[view.template.id])}
              busy={busy}
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
