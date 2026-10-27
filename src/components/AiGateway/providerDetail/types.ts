import type {
  GatewayEffectiveTemplatePolicy,
  GatewayModelMapping,
  GatewayProviderKey,
  GatewayProviderTemplate,
  GatewayProviderTemplateView,
  GatewayUpstreamProvider,
  ModelPrice,
} from "@/lib/aiGateway";

export type ProviderDetailDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  provider: GatewayUpstreamProvider | null;
  /**
   * Latest persisted snapshot of this provider. Only the five runtime health
   * fields of rows matching the trimmed `(local_model, upstream_model)` key are
   * merged into the local draft; user-editable fields and unsaved edits stay intact.
   */
  runtimeProvider?: GatewayUpstreamProvider | null;
  prices?: ModelPrice[];
  busy: boolean;
  onSave: (
    provider: GatewayUpstreamProvider,
    prices: ModelPrice[],
  ) => Promise<boolean | void>;
  onDelete?: (providerId: string) => Promise<boolean | void> | void;
  templates?: GatewayProviderTemplateView[];
  availableTags?: string[];
  onDeleteModel?: (providerId: string, upstreamModel: string) => void;
  onRestoreModel?: (providerId: string, upstreamModel: string) => void;
  /** Clear one auto-disabled mapping row's runtime state (no `enabled` change). */
  onReenableModel?: (
    providerId: string,
    localModel: string,
    upstreamModel: string,
  ) => void;
  /** Clear every auto-disabled row of this provider (no `enabled` change). */
  onReenableModels?: (providerId: string) => void;
  /** Clear one upstream key's runtime state (no `enabled` change). */
  onReenableKey?: (providerId: string, keyId: string) => void;
};

/** Editable key-pool row: persisted fields plus a client-only `isNew` marker. */
export type ProviderKeyDraft = GatewayProviderKey & { isNew?: boolean };

/** A mapping row keeps the raw context draft so invalid input survives. */
export type MappingDraft = GatewayModelMapping & { contextDraft?: string };

/** Values derived from the provider, templates and local draft after the null check. */
export type ProviderDetailDerived = {
  isEditing: boolean;
  isTemplateBound: boolean;
  boundTemplateView: GatewayProviderTemplateView | undefined;
  boundTemplate: GatewayProviderTemplate | undefined;
  effectivePolicy: GatewayEffectiveTemplatePolicy;
  bindingBroken: boolean;
  lastSyncText: string | null;
  ignoredModels: string[];
  autoDisabledModels: MappingDraft[];
};
