import type {
  GatewayModelPool,
  GatewayModelPoolCandidate,
  GatewayModelPoolStatus,
  GatewayProviderTemplateView,
  GatewayUpstreamProvider,
} from "@/lib/aiGateway";

export type ModelPoolCandidateStatus =
  GatewayModelPoolStatus["candidates"][number];

export type SourceGroup = {
  providerId: string;
  providerName: string;
  templateName: string | null;
  rows: Array<{ key: string; candidate: GatewayModelPoolCandidate }>;
};

export type ModelPoolDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  providers: GatewayUpstreamProvider[];
  /** Current and retired pool aliases that a fallback model must not reuse. */
  aliases?: string[];
  /** Loaded template views, used to resolve a provider's bound template name. */
  templates?: GatewayProviderTemplateView[];
  /** The saved pool being edited; `null`/absent creates a new pool. */
  pool?: GatewayModelPool | null;
  /** Latest nonsecret status for the edited pool, used only for reasons. */
  poolStatus?: GatewayModelPoolStatus | null;
  busy?: boolean;
  /** Persist the pool; a rejected promise keeps the draft and surfaces its message. */
  onSave: (pool: GatewayModelPool) => Promise<void>;
  /** Delete the pool; only shown on existing pools. */
  onDelete?: (poolId: string) => Promise<boolean | void> | boolean | void;
};
