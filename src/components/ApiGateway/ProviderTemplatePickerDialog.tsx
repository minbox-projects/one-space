import { useTranslation } from "react-i18next";
import {
  ArrowRight,
  Plus,
  Server,
  Sparkles,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type {
  GatewayProviderTemplate,
  GatewayProviderTemplateView,
  GatewayUpstreamProvider,
} from "@/lib/apiGateway";

export type ProviderTemplatePickerDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  templates: GatewayProviderTemplateView[];
  providers: GatewayUpstreamProvider[];
  busy: boolean;
  onSelectBlank: () => void;
  onSelectTemplate: (template: GatewayProviderTemplate) => void;
  onEditTemplate?: (template: GatewayProviderTemplate) => void;
  onNewTemplate?: () => void;
};

function brandAccent(templateId: string, name: string): string {
  const key = `${templateId} ${name}`.toLowerCase();
  if (key.includes("command")) {
    return "bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 border border-indigo-500/20";
  }
  return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20";
}

function protocolBadgeClass(protocol: string): string {
  return protocol === "responses"
    ? "bg-purple-500/10 text-purple-600 dark:text-purple-400 border border-purple-500/20"
    : "bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20";
}

export function ProviderTemplatePickerDialog({
  open,
  onOpenChange,
  templates,
  busy,
  onSelectBlank,
  onSelectTemplate,
  onNewTemplate,
}: ProviderTemplatePickerDialogProps) {
  const { t } = useTranslation();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="w-full sm:max-w-3xl lg:max-w-4xl max-h-[88vh] overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0"
        data-testid="api-gateway-template-picker-dialog"
      >
        <DialogHeader className="pl-6 pr-14 py-4 border-b bg-card/80 backdrop-blur-sm shrink-0">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-center gap-3 min-w-0">
              <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-2xs">
                <Server className="h-4.5 w-4.5" />
              </div>
              <div className="min-w-0">
                <DialogTitle className="truncate text-base font-semibold leading-5 text-foreground">
                  {t(
                    "apiGatewayAddUpstreamProviderDialogTitle",
                    t("apiGatewaySelectProviderTemplate", "Add Upstream Provider"),
                  )}
                </DialogTitle>
                <DialogDescription className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                  {t(
                    "apiGatewaySelectProviderTemplateDesc",
                    "Connect quickly via official or preset templates, or configure manually from a blank form.",
                  )}
                </DialogDescription>
              </div>
            </div>

            {/* 顶部操作栏辅助按钮 */}
            {onNewTemplate && (
              <div className="flex items-center gap-2 shrink-0">
                <button
                  type="button"
                  data-testid="template-picker-new-btn"
                  onClick={onNewTemplate}
                  disabled={busy}
                  className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/80 bg-background px-3 text-xs font-medium text-muted-foreground transition hover:bg-muted hover:text-foreground active:scale-98 disabled:opacity-50 shrink-0"
                >
                  <Sparkles className="h-3.5 w-3.5 text-primary" />
                  <span>{t("apiGatewayNewTemplate", "New template")}</span>
                </button>
              </div>
            )}
          </div>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 p-6">
          {/* 通道 1: 从空白表单创建 */}
          <button
            type="button"
            data-testid="template-picker-blank-btn"
            onClick={onSelectBlank}
            disabled={busy}
            className="group w-full rounded-2xl border-2 border-dashed border-primary/30 bg-primary/5 p-4 sm:p-5 text-left transition-all duration-200 hover:border-primary/60 hover:bg-primary/10 active:scale-[0.99]"
          >
            <div className="flex items-center justify-between gap-4">
              <div className="flex items-start gap-3.5 min-w-0">
                <div className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-primary text-primary-foreground shadow-xs transition group-hover:scale-105">
                  <Plus className="h-5 w-5" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="font-semibold text-sm text-foreground flex items-center gap-2">
                    <span>{t("apiGatewayBlankProviderPreset", "Create manually")}</span>
                    <span className="rounded-full bg-primary/15 px-2 py-0.2 text-[10px] font-medium text-primary">
                      {t("apiGatewayBlankProviderPresetQuick", "Manual")}
                    </span>
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground leading-relaxed">
                    {t(
                      "apiGatewayBlankProviderPresetDesc",
                      "Skip presets and configure provider endpoint, protocol, and model mappings manually.",
                    )}
                  </p>
                </div>
              </div>
              <div className="hidden sm:flex items-center gap-1 text-xs font-medium text-primary shrink-0 opacity-80 group-hover:opacity-100 group-hover:translate-x-0.5 transition">
                <span>{t("apiGatewayUseThisTemplate", "Configure")}</span>
                <ArrowRight className="h-3.5 w-3.5" />
              </div>
            </div>
          </button>

          {/* 分区提示栏 */}
          <div className="flex items-center justify-between pt-1 pb-0.5">
            <div className="flex items-center gap-2">
              <Sparkles className="h-3.5 w-3.5 text-primary" />
              <span className="text-xs font-semibold text-foreground">
                {t("apiGatewayPresetTemplatesSectionTitle", "Provider Templates")}
              </span>
              <span className="rounded-full bg-muted px-1.5 py-0.2 text-[10px] font-medium text-muted-foreground">
                {templates.length}
              </span>
            </div>
            <span className="text-[11px] text-muted-foreground hidden sm:inline">
              {t("apiGatewayPresetTemplatesHint", "Select a template to import official models and protocol configurations")}
            </span>
          </div>

          {/* 通道 2: 预设服务商模板列表 */}
          <div className="space-y-3">
            {templates.map((view) => {
              const { template } = view;
              const protocolLabel =
                template.protocol === "responses" ? "Responses" : "Chat";

              return (
                <button
                  type="button"
                  key={template.id}
                  data-testid={`template-picker-item-${template.id}`}
                  onClick={() => onSelectTemplate(template)}
                  disabled={busy}
                  className="group w-full rounded-2xl border border-border/80 bg-card p-4 sm:p-5 text-left shadow-2xs transition-all duration-200 hover:border-primary/40 hover:shadow-sm active:scale-[0.99]"
                >
                  <div className="flex items-center justify-between gap-4">
                    <div className="flex items-start gap-3.5 min-w-0 flex-1">
                      <div
                        className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-xl shadow-2xs transition group-hover:scale-105 ${brandAccent(template.id, template.name)}`}
                      >
                        <Sparkles className="h-5 w-5" />
                      </div>

                      <div className="min-w-0 flex-1 space-y-1.5">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-semibold text-sm text-foreground tracking-tight group-hover:text-primary transition-colors">
                            {template.name}
                          </span>
                          <span
                            className={`inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium leading-4 ${protocolBadgeClass(
                              template.protocol,
                            )}`}
                          >
                            {protocolLabel}
                          </span>
                        </div>

                        {template.description && (
                          <p className="line-clamp-2 text-xs text-muted-foreground leading-relaxed">
                            {template.description}
                          </p>
                        )}

                        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 pt-1 text-xs text-muted-foreground">
                          <span className="inline-flex items-center gap-1 font-medium text-foreground/80">
                            {t("apiGatewayTemplateModelsCount", {
                              count: template.models.length,
                              defaultValue: "{{count}} models",
                            })}
                          </span>
                          {template.source && (
                            <span className="font-mono text-[11px] break-all" title={template.source}>
                              {t("apiGatewayTemplateSource", "Source")}: {template.source}
                            </span>
                          )}
                        </div>
                      </div>
                    </div>

                    <div className="hidden sm:flex items-center gap-1 text-xs font-medium text-primary shrink-0 opacity-80 group-hover:opacity-100 group-hover:translate-x-0.5 transition">
                      <span>{t("apiGatewayUseThisTemplate", "Use template")}</span>
                      <ArrowRight className="h-3.5 w-3.5" />
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
