import type { GatewayEffectiveTemplatePolicy, GatewayModelMapping, GatewayProviderTemplate, GatewayProviderTemplateView, GatewayTemplateErrorStrategy, GatewayTemplateProviderRef, GatewayTemplateQuotaStrategy, GatewayTemplateSyncStrategy } from "./types";
import type { TFunction } from "i18next";

/** The generic fallback used by every unbound or unresolved provider binding. */
export const GATEWAY_GENERIC_TEMPLATE_POLICY: GatewayEffectiveTemplatePolicy = {
  sync_strategy: "generic",
  quota_strategy: "none",
  error_strategy: "generic",
};


/** Read a non-blank `template_id` from a loosely typed provider reference. */
function boundTemplateId(provider: GatewayTemplateProviderRef): string | null {
  if (!provider || typeof provider !== "object") return null;
  const raw = (provider as { template_id?: unknown }).template_id;
  return typeof raw === "string" && raw !== "" ? raw : null;
}


function normalizeTemplateSyncStrategy(value: unknown): GatewayTemplateSyncStrategy {
  return value === "opencode_go" ? "opencode_go" : "generic";
}


function normalizeTemplateQuotaStrategy(value: unknown): GatewayTemplateQuotaStrategy {
  if (value === "commandcode") return "commandcode";
  if (value === "opencode_go") return "opencode_go";
  return "none";
}


function normalizeTemplateErrorStrategy(): GatewayTemplateErrorStrategy {
  return "generic";
}


/**
 * Resolve the template a provider is bound to, strictly from its `template_id`.
 *
 * An unbound provider, an unknown or deleted id and an empty template list all
 * resolve to `null`; URL, name and icon are never consulted.
 */
export function resolveProviderTemplate(
  templates: GatewayProviderTemplateView[] | null | undefined,
  provider: GatewayTemplateProviderRef,
): GatewayProviderTemplateView | null {
  const templateId = boundTemplateId(provider);
  if (!templateId) return null;
  return (templates ?? []).find((view) => view.template.id === templateId) ?? null;
}


/**
 * Resolve a provider's effective policy from its bound template, falling back to
 * the generic policy for an unbound, unknown or deleted binding. Missing or
 * unknown stored strategy values resolve to `generic`/`none`/`generic`.
 */
export function resolveEffectiveTemplatePolicy(
  templates: GatewayProviderTemplateView[] | null | undefined,
  provider: GatewayTemplateProviderRef,
): GatewayEffectiveTemplatePolicy {
  const view = resolveProviderTemplate(templates, provider);
  if (!view) return { ...GATEWAY_GENERIC_TEMPLATE_POLICY };
  return {
    sync_strategy: normalizeTemplateSyncStrategy(view.template.sync_strategy),
    quota_strategy: normalizeTemplateQuotaStrategy(view.template.quota_strategy),
    error_strategy: normalizeTemplateErrorStrategy(),
  };
}


/**
 * Whether a provider carries a `template_id` that no longer resolves. An unbound
 * provider is never broken.
 */
export function isProviderTemplateBindingBroken(
  templates: GatewayProviderTemplateView[] | null | undefined,
  provider: GatewayTemplateProviderRef,
): boolean {
  const templateId = boundTemplateId(provider);
  if (!templateId) return false;
  return !(templates ?? []).some((view) => view.template.id === templateId);
}


/** Display order for weekday chips: Monday first, Sunday last. */
export const GATEWAY_WEEKDAY_ORDER: readonly number[] = [1, 2, 3, 4, 5, 6, 0];


const GATEWAY_WEEKDAY_KEYS = [
  "aiGatewayWeekdaySun",
  "aiGatewayWeekdayMon",
  "aiGatewayWeekdayTue",
  "aiGatewayWeekdayWed",
  "aiGatewayWeekdayThu",
  "aiGatewayWeekdayFri",
  "aiGatewayWeekdaySat",
] as const;


/** i18n key for a UTC+8 weekday (`0` Sunday–`6` Saturday); `""` when out of range. */
export function gatewayWeekdayTranslationKey(day: number): string {
  return GATEWAY_WEEKDAY_KEYS[day] ?? "";
}


/**
 * Render a weekday set as localized labels. Invalid entries are dropped and
 * duplicates collapsed; an empty result means every day.
 */
export function formatOffPeakDays(
  days: number[] | null | undefined,
  t: TFunction,
): string {
  const unique = Array.from(
    new Set((days ?? []).filter((day) => day >= 0 && day <= 6)),
  );
  if (unique.length === 0) return t("aiGatewayEveryDay");
  return GATEWAY_WEEKDAY_ORDER.filter((day) => unique.includes(day))
    .map((day) => t(gatewayWeekdayTranslationKey(day)))
    .join(", ");
}


/** Whether a mapping's upstream model is absent from the template's current models. */
export function isMappingDeprecated(
  mapping: GatewayModelMapping,
  template: GatewayProviderTemplate | null | undefined,
): boolean {
  if (!template) return false;
  return !template.models.some(
    (model) => model.upstream_model === mapping.upstream_model,
  );
}
