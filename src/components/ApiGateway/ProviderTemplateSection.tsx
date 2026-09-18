import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Brain,
  ChevronDown,
  ChevronUp,
  CloudOff,
  Loader2,
  Moon,
  Plus,
  RefreshCw,
  Sparkles,
} from "lucide-react";
import {
  formatGatewayTimestamp,
  formatOffPeakDays,
  type CreateProviderFromTemplateRequest,
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

function offPeakWindows(
  model: GatewayProviderTemplateModel,
): NonNullable<GatewayProviderTemplateModel["off_peaks"]> {
  if (model.off_peaks && model.off_peaks.length > 0) return model.off_peaks;
  return model.off_peak ? [model.off_peak] : [];
}

export function ProviderTemplateSection({
  templates,
  busy,
  syncingTemplateIds,
  onSync,
  onCreateProvider,
}: ProviderTemplateSectionProps) {
  const { t } = useTranslation();
  const [expandedIds, setExpandedIds] = useState<Record<string, boolean>>({});
  const [createView, setCreateView] = useState<GatewayProviderTemplateView | null>(
    null,
  );

  const toggleExpanded = (templateId: string) => {
    setExpandedIds((prev) => ({ ...prev, [templateId]: !prev[templateId] }));
  };

  return (
    <section className="space-y-3" data-testid="api-gateway-provider-templates">
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
            "Built-in catalogs of official models, prices and reasoning efforts.",
          )}
        </p>
      </div>

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
          {templates.map((view) => {
            const { template } = view;
            const expanded = Boolean(expandedIds[template.id]);
            const syncing = Boolean(syncingTemplateIds[template.id]);
            const protocolLabel =
              template.protocol === "responses" ? "Responses" : "Chat";
            const syncedText = view.synced_at
              ? formatGatewayTimestamp(view.synced_at)
              : t("apiGatewayTemplateNotSynced", "Not synced yet");

            return (
              <div
                key={template.id}
                data-testid={`api-gateway-template-${template.id}`}
                className="flex flex-col overflow-hidden rounded-xl border bg-card shadow-sm transition-all hover:border-primary/40 hover:shadow-md"
              >
                <div className="flex items-start gap-3 p-4">
                  <div
                    className={`rounded-lg p-2 ${brandAccent(template.id, template.name)}`}
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
                      {view.from_snapshot ? (
                        <>
                          <span
                            data-testid={`api-gateway-template-snapshot-${template.id}`}
                            className="inline-flex items-center gap-1 rounded-md border border-amber-500/25 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium leading-4 text-amber-700 dark:text-amber-400"
                          >
                            <CloudOff className="h-3 w-3" />
                            {t("apiGatewayTemplateSnapshot", "Offline snapshot")}
                          </span>
                          <span
                            data-testid={`api-gateway-template-snapshot-version-${template.id}`}
                            className="inline-flex items-center rounded-md border border-amber-500/25 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium leading-4 text-amber-700 dark:text-amber-400"
                          >
                            {t("apiGatewayTemplateSnapshotVersion", {
                              version: template.snapshot_version,
                              defaultValue: "Snapshot {{version}}",
                            })}
                          </span>
                        </>
                      ) : null}
                    </div>

                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                      {template.description}
                    </p>

                    <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
                      <span>
                        {t("apiGatewayTemplateModelsCount", {
                          count: template.models.length,
                          defaultValue: "{{count}} models",
                        })}
                      </span>
                      <span className="truncate" title={template.source}>
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

                  <div className="flex shrink-0 flex-col items-end gap-1.5">
                    <button
                      type="button"
                      data-testid={`api-gateway-template-sync-${template.id}`}
                      onClick={() => onSync(template.id)}
                      disabled={syncing}
                      aria-label={
                        syncing
                          ? t("apiGatewayTemplateSyncing", "Syncing...")
                          : t("apiGatewayTemplateSync", "Sync")
                      }
                      className="inline-flex h-7 items-center gap-1 rounded-md border bg-background px-2 text-[11px] font-medium shadow-sm transition hover:bg-muted disabled:opacity-60"
                    >
                      {syncing ? (
                        <Loader2 className="h-3 w-3 animate-spin" />
                      ) : (
                        <RefreshCw className="h-3 w-3" />
                      )}
                      {syncing
                        ? t("apiGatewayTemplateSyncing", "Syncing...")
                        : t("apiGatewayTemplateSync", "Sync")}
                    </button>

                    <button
                      type="button"
                      data-testid={`api-gateway-template-add-${template.id}`}
                      onClick={() => setCreateView(view)}
                      disabled={busy}
                      aria-label={t(
                        "apiGatewayTemplateAddProvider",
                        "Add as upstream provider",
                      )}
                      className="inline-flex h-7 items-center gap-1 rounded-md bg-primary px-2.5 text-[11px] font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
                    >
                      <Plus className="h-3 w-3" />
                      {t("apiGatewayTemplateAddProvider", "Add as upstream provider")}
                    </button>
                  </div>
                </div>

                <button
                  type="button"
                  data-testid={`api-gateway-template-expand-${template.id}`}
                  aria-expanded={expanded}
                  onClick={() => toggleExpanded(template.id)}
                  className="inline-flex h-7 items-center justify-center gap-1 border-t bg-muted/30 text-[11px] font-medium text-muted-foreground transition hover:bg-muted/60 hover:text-foreground"
                >
                  {expanded ? (
                    <ChevronUp className="h-3 w-3" />
                  ) : (
                    <ChevronDown className="h-3 w-3" />
                  )}
                  {expanded
                    ? t("apiGatewayTemplateCollapse", "Hide template models")
                    : t("apiGatewayTemplateExpand", "Show template models")}
                </button>

                {expanded ? (
                  <div className="max-h-64 overflow-y-auto border-t bg-muted/20 rounded-b-xl">
                    {template.models.length === 0 ? (
                      <p
                        data-testid={`api-gateway-template-no-models-${template.id}`}
                        className="p-3 text-[11px] text-muted-foreground"
                      >
                        {t("apiGatewayTemplateNoModels", "No models in this template")}
                      </p>
                    ) : (
                      template.models.map((model) => {
                        const peaks = offPeakWindows(model);
                        const efforts = model.reasoning_efforts ?? [];
                        return (
                          <div
                            key={model.upstream_model}
                            data-testid={`api-gateway-template-model-${template.id}-${model.upstream_model}`}
                            className="space-y-2 border-b border-border/50 p-3 last:border-b-0"
                          >
                            <div className="flex flex-wrap items-center gap-1.5">
                              <span className="font-mono text-xs font-semibold text-foreground">
                                {model.upstream_model}
                              </span>
                              {model.display_name ? (
                                <span className="text-[11px] text-muted-foreground">
                                  {model.display_name}
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

                            {/* 四档标准价 */}
                            <div className="grid grid-cols-4 gap-1.5">
                              {[
                                {
                                  label: t("apiGatewayTemplatePriceInput", "Input"),
                                  value: model.input,
                                  className: "text-foreground",
                                },
                                {
                                  label: t(
                                    "apiGatewayTemplatePriceCacheRead",
                                    "Cache read",
                                  ),
                                  value: model.cache_read,
                                  className: "text-muted-foreground",
                                },
                                {
                                  label: t(
                                    "apiGatewayTemplatePriceCacheWrite",
                                    "Cache write",
                                  ),
                                  value: model.cache_write,
                                  className: "text-muted-foreground",
                                },
                                {
                                  label: t("apiGatewayTemplatePriceOutput", "Output"),
                                  value: model.output,
                                  className: "text-foreground",
                                },
                              ].map((tier) => (
                                <div
                                  key={tier.label}
                                  className="rounded-md border bg-background/60 px-1.5 py-1"
                                >
                                  <div className="truncate text-[10px] text-muted-foreground">
                                    {tier.label}
                                  </div>
                                  <div className={`font-mono text-xs ${tier.className}`}>
                                    {tier.value}
                                  </div>
                                </div>
                              ))}
                            </div>

                            {/* 峰谷时段 */}
                            {peaks.length === 0 ? (
                              <p className="text-[10px] text-muted-foreground">
                                {t(
                                  "apiGatewayTemplateNoOffPeak",
                                  "No off-peak windows",
                                )}
                              </p>
                            ) : (
                              <div className="space-y-1">
                                {peaks.map((peak, index) => (
                                  <div
                                    key={`${peak.start_time}-${peak.end_time}-${index}`}
                                    className="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-md border border-amber-500/20 bg-amber-500/5 px-2 py-1 text-[10px] text-amber-700 dark:text-amber-400"
                                  >
                                    <span className="inline-flex items-center gap-1 font-medium">
                                      <Moon className="h-3 w-3" />
                                      {t("apiGatewayTemplateOffPeak", "Off-peak")}
                                    </span>
                                    <span className="font-mono">
                                      {peak.start_time} - {peak.end_time}
                                    </span>
                                    <span>{formatOffPeakDays(peak.days, t)}</span>
                                    <span className="flex items-center gap-1.5 font-mono">
                                      <span>
                                        {t("apiGatewayTemplatePriceInput", "Input")}{" "}
                                        {peak.input}
                                      </span>
                                      <span>
                                        {t(
                                          "apiGatewayTemplatePriceCacheRead",
                                          "Cache read",
                                        )}{" "}
                                        {peak.cache_read}
                                      </span>
                                      <span>
                                        {t(
                                          "apiGatewayTemplatePriceCacheWrite",
                                          "Cache write",
                                        )}{" "}
                                        {peak.cache_write}
                                      </span>
                                      <span>
                                        {t(
                                          "apiGatewayTemplatePriceOutput",
                                          "Output",
                                        )}{" "}
                                        {peak.output}
                                      </span>
                                    </span>
                                  </div>
                                ))}
                              </div>
                            )}

                            {/* reasoning_efforts chips */}
                            {efforts.length > 0 ? (
                              <div className="flex flex-wrap items-center gap-1">
                                <span className="inline-flex items-center gap-1 text-[10px] text-muted-foreground">
                                  <Brain className="h-3 w-3" />
                                  {t(
                                    "apiGatewayTemplateReasoningEfforts",
                                    "Reasoning efforts",
                                  )}
                                </span>
                                {efforts.map((effort) => (
                                  <span
                                    key={effort}
                                    className="inline-flex items-center rounded-full bg-secondary px-2 py-0.5 text-[10px] font-medium text-secondary-foreground"
                                  >
                                    {effort}
                                  </span>
                                ))}
                              </div>
                            ) : null}
                          </div>
                        );
                      })
                    )}
                  </div>
                ) : null}
              </div>
            );
          })}
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
