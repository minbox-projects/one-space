import { AlertCircle, Clock } from "lucide-react";
import { CustomSelect } from "@/components/common/CustomSelect";
import { ProviderTemplateAvatar } from "../ProviderTemplateIcon";
import {
  ERROR_STRATEGY_LABEL_KEYS,
  QUOTA_STRATEGY_LABEL_KEYS,
  SYNC_STRATEGY_LABEL_KEYS,
} from "./helpers";
import type { ProviderDetailDerived } from "./types";
import type { ProviderDetailForm } from "./hooks/useProviderDetailForm";

export function ProviderTemplateBannerSection({
  form,
  derived,
}: {
  form: ProviderDetailForm;
  derived: ProviderDetailDerived;
}) {
  const {
    t,
    provider,
    templates,
    templateId,
    setTemplateId,
  } = form;
  const {
    isTemplateBound,
    boundTemplate,
    effectivePolicy,
    bindingBroken,
    lastSyncText,
  } = derived;
  return (
    <>
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

          {/* 模板绑定与有效策略展示（无服务商级策略覆盖控件） */}
          <div className="space-y-2 rounded-xl border border-border/70 bg-muted/20 px-3.5 py-2.5">
            <div className="flex flex-wrap items-end justify-between gap-3">
              <div className="field min-w-[220px] flex-1">
                <label>
                  {t("aiGatewayProviderTemplateBinding", "Provider template")}
                </label>
                <CustomSelect
                  selectTestId="ai-gateway-provider-template-select"
                  triggerTestId="ai-gateway-provider-template-trigger"
                  value={templateId}
                  onChange={(val) => setTemplateId(val)}
                  ariaLabel={t(
                    "aiGatewayProviderTemplateBinding",
                    "Provider template",
                  )}
                  options={[
                    {
                      value: "",
                      label: t(
                        "aiGatewayProviderTemplateUnbound",
                        "No template (generic)",
                      ),
                    },
                    ...(templates ?? []).map((view) => ({
                      value: view.template.id,
                      label: view.template.name,
                    })),
                  ]}
                />
              </div>
              <div
                data-testid="ai-gateway-provider-effective-strategy"
                data-sync-strategy={effectivePolicy.sync_strategy}
                data-quota-strategy={effectivePolicy.quota_strategy}
                data-error-strategy={effectivePolicy.error_strategy}
                className="rounded-lg border border-border/60 bg-background px-2.5 py-1.5 text-[11px] text-muted-foreground"
              >
                <span className="font-medium text-foreground">
                  {t("aiGatewayProviderEffectiveStrategy", "Effective strategy")}:
                </span>{" "}
                <span>
                  {t("aiGatewayTemplateSyncStrategy")}=
                  {t(SYNC_STRATEGY_LABEL_KEYS[effectivePolicy.sync_strategy])}
                </span>{" "}
                <span>
                  {t("aiGatewayTemplateQuotaStrategy")}=
                  {t(QUOTA_STRATEGY_LABEL_KEYS[effectivePolicy.quota_strategy])}
                </span>{" "}
                <span>
                  {t("aiGatewayTemplateErrorStrategy")}=
                  {t(ERROR_STRATEGY_LABEL_KEYS[effectivePolicy.error_strategy])}
                </span>
              </div>
            </div>
            {bindingBroken ? (
              <div
                data-testid="ai-gateway-provider-broken-binding-warning"
                role="alert"
                className="flex items-center gap-1.5 rounded-lg border border-destructive/30 bg-destructive/10 px-2.5 py-1.5 text-[11px] text-destructive"
              >
                <AlertCircle className="h-3.5 w-3.5 shrink-0" />
                <span>
                  {t(
                    "aiGatewayProviderTemplateBrokenBinding",
                    "The bound template no longer exists; this provider uses the generic strategy until you rebind or unbind it.",
                  )}
                </span>
              </div>
            ) : null}
          </div>
    </>
  );
}
