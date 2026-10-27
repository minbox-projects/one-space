import type {
  GatewayProviderKeyState,
  GatewayTemplateErrorStrategy,
  GatewayTemplateQuotaStrategy,
  GatewayTemplateSyncStrategy,
} from "@/lib/aiGateway";
import type { ProviderKeyDraft } from "./types";

export const mappingInputClass =
  "h-[38px] rounded-lg border border-border bg-background px-3 text-sm text-foreground outline-none transition-all focus:border-primary focus:ring-2 focus:ring-primary/50";

export const SYNC_STRATEGY_LABEL_KEYS: Record<GatewayTemplateSyncStrategy, string> = {
  generic: "aiGatewayTemplateSyncStrategyGeneric",
  opencode_go: "aiGatewayTemplateSyncStrategyOpencodeGo",
};

export const QUOTA_STRATEGY_LABEL_KEYS: Record<GatewayTemplateQuotaStrategy, string> = {
  none: "aiGatewayTemplateQuotaStrategyNone",
  commandcode: "aiGatewayTemplateQuotaStrategyCommandcode",
  opencode_go: "aiGatewayTemplateQuotaStrategyOpencodeGo",
};

export const ERROR_STRATEGY_LABEL_KEYS: Record<GatewayTemplateErrorStrategy, string> = {
  generic: "aiGatewayTemplateErrorStrategyGeneric",
};

/** Stable identity for a mapping row's trimmed `(local_model, upstream_model)` key. */
export function runtimeMappingKey(localModel: string, upstreamModel: string): string {
  return JSON.stringify([localModel.trim(), upstreamModel.trim()]);
}

let providerKeyDraftCounter = 0;

/** Build a blank new-key draft with a client-generated stable id. */
export function newProviderKeyDraft(): ProviderKeyDraft {
  providerKeyDraftCounter += 1;
  return {
    id: `new-key-${Date.now()}-${providerKeyDraftCounter}`,
    name: "",
    value: "",
    enabled: true,
    daily_reset: false,
    auto_marked: false,
    failure_kind: null,
    marked_at: null,
    reason: null,
    isNew: true,
  };
}

export const providerKeyStateClass: Record<GatewayProviderKeyState, string> = {
  usable:
    "border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
  disabled: "border-border bg-muted text-muted-foreground",
  quota:
    "border-amber-500/20 bg-amber-500/10 text-amber-600 dark:text-amber-400",
  auth: "border-rose-500/20 bg-rose-500/10 text-rose-600 dark:text-rose-400",
};

export const providerKeyStateDotClass: Record<GatewayProviderKeyState, string> = {
  usable: "bg-emerald-500 shadow-[0_0_6px_rgba(16,185,129,0.5)]",
  disabled: "bg-muted-foreground/40",
  quota: "bg-amber-500 shadow-[0_0_6px_rgba(245,158,11,0.5)] animate-pulse",
  auth: "bg-rose-500 shadow-[0_0_6px_rgba(244,63,94,0.5)] animate-pulse",
};

export const providerKeyIconContainerClass: Record<GatewayProviderKeyState, string> = {
  usable:
    "border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 shadow-2xs",
  disabled: "border-border/60 bg-muted/80 text-muted-foreground/60",
  quota:
    "border-amber-500/20 bg-amber-500/10 text-amber-600 dark:text-amber-400 shadow-2xs",
  auth: "border-rose-500/20 bg-rose-500/10 text-rose-600 dark:text-rose-400 shadow-2xs",
};

export function maskSecret(value?: string): string {
  if (!value) return "••••••••••••";
  if (value.length <= 8) return "••••••••";
  return `${value.slice(0, 4)}••••${value.slice(-4)}`;
}
