import { Info } from "lucide-react";
import { CustomSelect } from "@/components/common/CustomSelect";
import {
  PROVIDER_CUSTOM_ICON_OPTIONS,
  ProviderTemplateIconPicker,
  resolveEffectiveProviderIcon,
} from "../ProviderTemplateIcon";
import { mappedUpstreamModels, type GatewayUpstreamProtocol } from "@/lib/aiGateway";
import type { ProviderDetailDerived } from "./types";
import type { ProviderDetailForm } from "./hooks/useProviderDetailForm";

export function ProviderBasicFieldsSection({
  form,
  derived,
}: {
  form: ProviderDetailForm;
  derived: ProviderDetailDerived;
}) {
  const {
    t,
    name,
    setName,
    icon,
    setIcon,
    protocol,
    setProtocol,
    defaultModel,
    setDefaultModel,
    baseUrl,
    setBaseUrl,
    weight,
    setWeight,
    provider,
    mappings,
  } = form;
  const {
    isTemplateBound,
    boundTemplate,
  } = derived;
  return (
    <>
            {/* 第 1 行：名称与自定义图标并排 */}
            <div className="field">
              <label className="required">{t("aiGatewayName", "Name")}</label>
              <input
                type="text"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="e.g. DeepSeek / OpenAI"
                aria-label={t("aiGatewayName", "Name")}
              />
            </div>

            <div className="field">
              <label className="inline-flex items-center gap-1.5">
                <span>{t("aiGatewayCustomIcon", "Provider icon")}</span>
                <span className="relative group inline-flex items-center">
                  <Info
                    className="h-3.5 w-3.5 cursor-help text-muted-foreground/70 transition hover:text-foreground"
                    aria-label={t("aiGatewayCustomIconDesc", "Select a custom icon or inherit from the bound template.")}
                  />
                  <span
                    role="tooltip"
                    className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-56 rounded-md border bg-popover p-2 text-left text-xs font-normal text-popover-foreground shadow-lg group-hover:block group-focus-within:block"
                  >
                    {t(
                      "aiGatewayCustomIconDesc",
                      "Select a custom icon or inherit from the bound template.",
                    )}
                  </span>
                </span>
              </label>
              <ProviderTemplateIconPicker
                value={icon}
                onChange={setIcon}
                options={PROVIDER_CUSTOM_ICON_OPTIONS}
                inheritedIcon={
                  boundTemplate?.icon ||
                  resolveEffectiveProviderIcon(
                    { template_id: provider?.template_id, base_url: baseUrl, name, id: provider?.id },
                    boundTemplate,
                  )
                }
                autoLabel={
                  isTemplateBound
                    ? t("aiGatewayInheritTemplateIcon", "Inherit from template (Default)")
                    : t("aiGatewayDefaultIcon", "Default icon")
                }
                templateId={(boundTemplate?.id ?? provider?.template_id) || undefined}
                templateName={name || boundTemplate?.name}
                baseUrl={baseUrl}
                triggerTestId="provider-edit-icon-trigger"
                selectTestId="provider-edit-icon"
                menuTestId="provider-edit-icon-menu"
              />
            </div>

            {/* 第 2 行：接口协议与默认模型并排 */}
            <div className="field">
              <label className="required">{t("aiGatewayProtocol", "API protocol")}</label>
              <CustomSelect<GatewayUpstreamProtocol>
                value={protocol}
                onChange={(val) => setProtocol(val)}
                ariaLabel={t("aiGatewayProtocol", "API protocol")}
                selectTestId="ai-gateway-protocol-select"
                triggerTestId="ai-gateway-protocol-trigger"
                options={[
                  {
                    value: "chat_completions",
                    label: t(
                      "aiGatewayProtocolChat",
                      "Chat Completions (/chat/completions)",
                    ),
                  },
                  {
                    value: "responses",
                    label: t(
                      "aiGatewayProtocolResponses",
                      "Responses (/responses)",
                    ),
                  },
                  {
                    value: "anthropic_messages",
                    label: t(
                      "aiGatewayProtocolAnthropicMessages",
                      "Anthropic Messages (native)",
                    ),
                  },
                ]}
              />
            </div>

            <div className="field">
              <label className="inline-flex items-center gap-1.5">
                <span>{t("aiGatewayDefaultModel", "Default model")}</span>
                <span className="relative group inline-flex items-center">
                  <Info
                    className="h-3.5 w-3.5 cursor-help text-muted-foreground/70 transition hover:text-foreground"
                    aria-label={t("aiGatewayDefaultModelHint", "Enter the real model name on the upstream provider. Used as a fallback when no mapping matches.")}
                  />
                  <span
                    role="tooltip"
                    className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-56 rounded-md border bg-popover p-2 text-left text-xs font-normal text-popover-foreground shadow-lg group-hover:block group-focus-within:block"
                  >
                    {t(
                      "aiGatewayDefaultModelHint",
                      "Enter the real model name on the upstream provider. Used as a fallback when no mapping matches.",
                    )}
                  </span>
                </span>
              </label>
              <CustomSelect
                selectTestId="ai-gateway-default-model-select"
                triggerTestId="ai-gateway-default-model-trigger"
                value={defaultModel}
                onChange={(val) => setDefaultModel(val)}
                ariaLabel={t("aiGatewayDefaultModel", "Default model")}
                triggerClassName="font-mono text-xs"
                options={[
                  {
                    value: "",
                    label: t("aiGatewayDefaultModelNone", "None"),
                  },
                  ...mappedUpstreamModels({ ...provider, mappings }).map((model) => ({
                    value: model,
                    label: model,
                  })),
                ]}
              />
            </div>

            {/* 第 3 行：API Base URL 独占一行 */}
            <div className="field full-span">
              <label className="required">{t("aiGatewayBaseUrl", "API base URL")}</label>
              <input
                type="text"
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
                placeholder="https://api.openai.com"
                aria-label={t("aiGatewayBaseUrl", "API base URL")}
                className="font-mono"
              />
            </div>

            {/* 路由权重独占一行 */}
            <div className="field full-span">
              <label className="inline-flex items-center gap-1.5">
                <span>{t("aiGateway.provider.weight", "Weight")}</span>
                <span className="relative group inline-flex items-center">
                  <Info
                    className="h-3.5 w-3.5 cursor-help text-muted-foreground/70 transition hover:text-foreground"
                    aria-label={t(
                      "aiGateway.provider.weightHint",
                      "Higher weight forwards requests more frequently (1-100)",
                    )}
                  />
                  <span
                    role="tooltip"
                    className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-64 rounded-md border bg-popover p-2 text-left text-xs font-normal text-popover-foreground shadow-lg group-hover:block group-focus-within:block"
                  >
                    {t(
                      "aiGateway.provider.weightHint",
                      "Higher weight forwards requests more frequently (1-100)",
                    )}
                  </span>
                </span>
              </label>
              <input
                data-testid="ai-gateway-provider-weight-input"
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
                aria-label={t("aiGateway.provider.weight", "Weight")}
              />
              <p className="text-[11px] text-muted-foreground mt-1">
                {t(
                  "aiGateway.provider.weightHint",
                  "Higher weight forwards requests more frequently (1-100)",
                )}
              </p>
            </div>
    </>
  );
}
