import { AlertCircle, Server, Sparkles, Trash2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  formatGatewayTimestamp,
  isProviderTemplateBindingBroken,
  resolveEffectiveTemplatePolicy,
} from "@/lib/aiGateway";
import {
  ProviderTemplateAvatar,
  resolveEffectiveProviderIcon,
} from "./ProviderTemplateIcon";
import { ProviderKeyDialog } from "./ProviderKeyDialog";
import { ProviderBasicFieldsSection } from "./providerDetail/ProviderBasicFieldsSection";
import { ProviderKeysSection } from "./providerDetail/ProviderKeysSection";
import { ProviderMappingsSection } from "./providerDetail/ProviderMappingsSection";
import { ProviderTemplateBannerSection } from "./providerDetail/ProviderTemplateBannerSection";
import { useProviderDetailForm } from "./providerDetail/hooks/useProviderDetailForm";
import type {
  ProviderDetailDerived,
  ProviderDetailDialogProps,
} from "./providerDetail/types";

export function ProviderDetailDialog(props: ProviderDetailDialogProps) {
  const form = useProviderDetailForm(props);
  const {
    open,
    onOpenChange,
    provider,
    busy,
    onDelete,
    name,
    baseUrl,
    icon,
    t,
    templateId,
    templates,
    mappings,
    confirmDeleting,
    handleSave,
    handleDelete,
    handleCancelDelete,
    handleConfirmDelete,
    keyModalOpen,
    setKeyModalOpen,
    editingKeyIndex,
    keyDrafts,
    handleKeyModalConfirm,
    handleKeyModalDelete,
  } = form;

  if (!provider) return null;

  const isEditing = Boolean(provider.id);
  const isTemplateBound = Boolean(provider.template_id);
  const boundTemplateView = provider.template_id
    ? templates?.find((view) => view.template.id === provider.template_id)
    : undefined;
  const boundTemplate = boundTemplateView?.template;
  const effectivePolicy = resolveEffectiveTemplatePolicy(templates, {
    template_id: templateId,
  });
  const bindingBroken = isProviderTemplateBindingBroken(templates, {
    template_id: templateId,
  });
  const lastSyncText = boundTemplateView?.synced_at
    ? formatGatewayTimestamp(boundTemplateView.synced_at)
    : t("aiGatewayTemplateNotSynced", "Not synced yet");
  const ignoredModels = isTemplateBound ? provider.ignored_models ?? [] : [];
  // 从本地草稿派生自动禁用计数与批量重新启用控件，使合并后的运行时更新立即反映。
  const autoDisabledModels = mappings.filter(
    (mapping) => mapping.auto_disabled === true,
  );
  const derived: ProviderDetailDerived = {
    isEditing,
    isTemplateBound,
    boundTemplateView,
    boundTemplate,
    effectivePolicy,
    bindingBroken,
    lastSyncText,
    ignoredModels,
    autoDisabledModels,
  };

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[90vh] w-full sm:max-w-6xl overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0"
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
          <ProviderTemplateBannerSection form={form} derived={derived} />

          <div className="field-grid col-2 mb-0">
            <ProviderBasicFieldsSection form={form} derived={derived} />

            <ProviderKeysSection form={form} />
          </div>

          <ProviderMappingsSection form={form} derived={derived} />
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
              onClick={() => void handleSave()}
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
            className="absolute inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm p-4 animate-in fade-in-0 duration-150 sm:rounded-2xl"
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

    <ProviderKeyDialog
      open={keyModalOpen}
      onOpenChange={setKeyModalOpen}
      keyData={
        editingKeyIndex !== null ? keyDrafts[editingKeyIndex] ?? null : null
      }
      isNew={
        editingKeyIndex === null ||
        Boolean(keyDrafts[editingKeyIndex]?.isNew)
      }
      onConfirm={handleKeyModalConfirm}
      onDelete={editingKeyIndex !== null ? handleKeyModalDelete : undefined}
    />
  </>
  );
}
