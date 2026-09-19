import { useTranslation } from "react-i18next";
import {
  Pencil,
  Plus,
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
  onEditTemplate: (template: GatewayProviderTemplate) => void;
  onNewTemplate?: () => void;
};

function brandAccent(templateId: string, name: string): string {
  const key = `${templateId} ${name}`.toLowerCase();
  if (key.includes("command")) {
    return "bg-indigo-500/10 text-indigo-600 dark:text-indigo-400";
  }
  return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400";
}

function protocolBadgeClass(protocol: string): string {
  return protocol === "responses"
    ? "bg-purple-500/10 text-purple-600 dark:text-purple-400"
    : "bg-blue-500/10 text-blue-600 dark:text-blue-400";
}

export function ProviderTemplatePickerDialog({
  open,
  onOpenChange,
  templates,
  busy,
  onSelectBlank,
  onSelectTemplate,
  onEditTemplate,
  onNewTemplate,
}: ProviderTemplatePickerDialogProps) {
  const { t } = useTranslation();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="w-full p-5 sm:max-w-2xl sm:rounded-xl max-h-[85vh] flex flex-col"
        data-testid="api-gateway-template-picker-dialog"
      >
        <DialogHeader className="space-y-1 pr-9">
          <div className="flex items-center justify-between gap-3">
            <DialogTitle className="text-base font-semibold">
              {t("apiGatewaySelectProviderTemplate", "Select provider preset")}
            </DialogTitle>
            {onNewTemplate && (
              <button
                type="button"
                data-testid="template-picker-new-btn"
                onClick={onNewTemplate}
                disabled={busy}
                className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50 shrink-0"
              >
                <Plus className="h-3.5 w-3.5" />
                {t("apiGatewayNewTemplate", "New template")}
              </button>
            )}
          </div>
          <DialogDescription className="text-xs text-muted-foreground">
            {t(
              "apiGatewaySelectProviderTemplateDesc",
              "Create an upstream provider quickly from an official catalog preset, or configure from a blank form.",
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-3 py-2 pr-1">
          {/* 选项 1: 从空白表单创建 */}
          <button
            type="button"
            data-testid="template-picker-blank-btn"
            onClick={onSelectBlank}
            disabled={busy}
            className="w-full rounded-xl border border-dashed border-primary/40 bg-primary/5 p-3.5 text-left transition hover:border-primary/60 hover:bg-primary/10"
          >
            <div className="flex items-start gap-3">
              <div className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground shadow-xs">
                <Plus className="h-4 w-4" />
              </div>
              <div className="min-w-0 flex-1">
                <div className="font-medium text-xs sm:text-sm text-foreground">
                  {t("apiGatewayBlankProviderPreset", "Create manually")}
                </div>
                <p className="mt-0.5 text-xs text-muted-foreground">
                  {t(
                    "apiGatewayBlankProviderPresetDesc",
                    "Skip presets and configure provider endpoint, protocol, and model mappings manually.",
                  )}
                </p>
              </div>
            </div>
          </button>

          {/* 选项 2: 预设服务商模板列表 */}
          {templates.map((view) => {
            const { template } = view;
            const protocolLabel =
              template.protocol === "responses" ? "Responses" : "Chat";

            return (
              <div
                key={template.id}
                data-testid={`template-picker-item-${template.id}`}
                className="flex items-start justify-between gap-2.5 rounded-xl border bg-card p-3.5 shadow-xs transition hover:border-primary/40 hover:shadow-sm"
              >
                <button
                  type="button"
                  data-testid={`template-picker-select-${template.id}`}
                  onClick={() => onSelectTemplate(template)}
                  disabled={busy}
                  className="min-w-0 flex-1 text-left flex items-start gap-3"
                >
                  <div
                    className={`rounded-lg p-2 shrink-0 ${brandAccent(template.id, template.name)}`}
                  >
                    <Sparkles className="h-4 w-4" />
                  </div>

                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <span className="font-medium text-xs sm:text-sm text-foreground">
                        {template.name}
                      </span>
                      <span
                        className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium leading-3 ${protocolBadgeClass(
                          template.protocol,
                        )}`}
                      >
                        {protocolLabel}
                      </span>
                    </div>

                    {template.description && (
                      <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                        {template.description}
                      </p>
                    )}

                    <div className="mt-2 flex flex-wrap items-center gap-x-3 text-[11px] text-muted-foreground">
                      <span>
                        {t("apiGatewayTemplateModelsCount", {
                          count: template.models.length,
                          defaultValue: "{{count}} models",
                        })}
                      </span>
                      {template.source && (
                        <span className="truncate max-w-[200px]" title={template.source}>
                          {t("apiGatewayTemplateSource", "Source")}: {template.source}
                        </span>
                      )}
                    </div>
                  </div>
                </button>

                {/* 右侧编辑按钮 */}
                <button
                  type="button"
                  data-testid={`template-picker-edit-${template.id}`}
                  onClick={() => onEditTemplate(template)}
                  disabled={busy}
                  title={t("apiGatewayEditTemplate", "Edit template")}
                  aria-label={t("apiGatewayEditTemplate", "Edit template")}
                  className="rounded-lg border p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground shrink-0"
                >
                  <Pencil className="h-3.5 w-3.5" />
                </button>
              </div>
            );
          })}
        </div>
      </DialogContent>
    </Dialog>
  );
}
