import { CANONICAL_REASONING_EFFORTS } from "./types";
import type { CanonicalReasoningEffort, GatewayModelMapping, GatewayModelPool, GatewayModelPoolCandidate, GatewayModelPoolCandidateStatus, GatewayModelPoolReasoning, GatewayProviderKey, GatewayUpstreamProtocol, GatewayUpstreamProvider, GatewayUpstreamProviderWithKeys, ModelPoolReasoningCatalog, ModelPoolReasoningCoverage, ModelPoolReasoningMapping, ModelPoolReasoningProfile, ResolvedModelPoolCandidateState } from "./types";

const MODEL_POOL_REASONING_PROFILES: ReadonlySet<string> = new Set([
  "openai_effort",
  "anthropic_effort_inherit",
  "anthropic_effort_adaptive",
  "anthropic_manual_budget",
  "chat_thinking_switch",
]);


/** Profiles whose positive action is a native enum value rather than a budget. */
const MODEL_POOL_ENUM_REASONING_PROFILES: ReadonlySet<string> = new Set([
  "openai_effort",
  "anthropic_effort_inherit",
  "anthropic_effort_adaptive",
  "chat_thinking_switch",
]);


/** Internal minimum budget an Anthropic manual-budget mapping may declare. */
export const MODEL_POOL_MIN_REASONING_BUDGET = 1024;


/** Whether a stored profile identifier is one of the bounded supported shapes. */
export function isKnownModelPoolReasoningProfile(
  profile: string | null | undefined,
): profile is ModelPoolReasoningProfile {
  return typeof profile === "string" && MODEL_POOL_REASONING_PROFILES.has(profile);
}


/**
 * Whether a declared profile may target one stored effective protocol. The
 * bounded profiles are protocol-family specific, so a profile on the wrong
 * ingress family never contributes a mapping.
 */
export function isModelPoolReasoningProfileSupportedOnProtocol(
  profile: string | null | undefined,
  protocol: GatewayUpstreamProtocol,
): boolean {
  switch (profile) {
    case "openai_effort":
      return protocol === "chat_completions" || protocol === "responses";
    case "chat_thinking_switch":
      return protocol === "chat_completions";
    case "anthropic_effort_inherit":
    case "anthropic_effort_adaptive":
    case "anthropic_manual_budget":
      return protocol === "anthropic_messages";
    default:
      return false;
  }
}


/**
 * Whether one candidate-local mapping is structurally valid for its declared
 * profile. Unknown profiles and unsupported action shapes never produce a
 * mapping, so no support is inferred from source metadata.
 */
export function isModelPoolReasoningMappingValid(
  reasoning: GatewayModelPoolReasoning | null | undefined,
  mapping: ModelPoolReasoningMapping,
): boolean {
  const profile = reasoning?.profile;
  if (!isKnownModelPoolReasoningProfile(profile)) return false;
  const effort = mapping.effort;
  if (!(CANONICAL_REASONING_EFFORTS as readonly string[]).includes(effort)) {
    return false;
  }
  const action = mapping.action;
  if (!action || typeof action.kind !== "string") return false;

  if (action.kind === "disable") {
    return effort === "none" && reasoning?.supports_disable === true;
  }
  if (effort === "none") return false;

  if (action.kind === "enum") {
    // Only an enum-capable profile may carry a positive native value; a
    // manual-budget candidate never does.
    if (!MODEL_POOL_ENUM_REASONING_PROFILES.has(profile)) return false;
    if (typeof action.value !== "string") return false;
    const trimmed = action.value.trim();
    if (trimmed === "") return false;
    // A positive preference must never target the reserved disable/omission
    // value.
    if (trimmed.toLowerCase() === "none") return false;
    // A switch profile selects one intensity: its positive value is `enabled`.
    if (profile === "chat_thinking_switch") return trimmed === "enabled";
    // An effort-profile value must stay inside the candidate's declared set.
    return (reasoning?.values ?? []).includes(action.value);
  }
  if (action.kind === "budget") {
    if (profile !== "anthropic_manual_budget") return false;
    const min = reasoning?.budget?.min;
    const max = reasoning?.budget?.max;
    if (typeof min !== "number" || typeof max !== "number") return false;
    if (!Number.isInteger(min) || !Number.isInteger(max)) return false;
    if (min < MODEL_POOL_MIN_REASONING_BUDGET || max < min) return false;
    const tokens = action.budget_tokens;
    if (typeof tokens !== "number" || !Number.isInteger(tokens)) return false;
    return tokens >= min && tokens <= max;
  }
  return false;
}


/** The concrete pool default preference, mapping a missing value to `auto`. */
export function resolveModelPoolDefaultEffort(
  pool: Pick<GatewayModelPool, "default_reasoning_effort"> | null | undefined,
): string {
  const value = pool?.default_reasoning_effort;
  return typeof value === "string" && value.trim() !== "" ? value : "auto";
}


/**
 * Whether one candidate's exact source reference currently resolves against the
 * given providers: the provider id, the original trimmed mapping pair and the
 * mapping's effective protocol. Availability and key state never enter.
 */
export function modelPoolCandidateResolves(
  providers: GatewayUpstreamProvider[] | null | undefined,
  candidate: Pick<
    GatewayModelPoolCandidate,
    "provider_id" | "local_model" | "upstream_model" | "protocol"
  >,
): boolean {
  return (providers ?? []).some(
    (provider) =>
      provider.id === candidate.provider_id &&
      (provider.mappings ?? []).some(
        (mapping) =>
          mapping.local_model.trim() === candidate.local_model &&
          mapping.upstream_model.trim() === candidate.upstream_model &&
          (mapping.protocol ?? provider.protocol ?? "chat_completions") ===
            candidate.protocol,
      ),
  );
}


/**
 * Whether one candidate statically serves `effort`: its exact source reference
 * resolves, its profile is representable on its effective protocol and it
 * declares a structurally valid mapping for the preference. Runtime readiness
 * and key state are deliberately excluded.
 */
export function modelPoolCandidateServesEffort(
  providers: GatewayUpstreamProvider[] | null | undefined,
  candidate: GatewayModelPoolCandidate,
  effort: string,
): boolean {
  const reasoning = candidate.reasoning;
  if (!reasoning) return false;
  const mapping = (reasoning.mappings ?? []).find(
    (item) => item.effort === effort,
  );
  if (!mapping) return false;
  if (!modelPoolCandidateResolves(providers, candidate)) return false;
  if (
    !isModelPoolReasoningProfileSupportedOnProtocol(
      reasoning.profile,
      candidate.protocol,
    )
  ) {
    return false;
  }
  return isModelPoolReasoningMappingValid(reasoning, mapping);
}


/** Map one source-advertised native token to a canonical pool preference. */
function fallbackCanonicalEffort(raw: string): CanonicalReasoningEffort | null {
  const normalized = raw.trim().toLowerCase();
  return (CANONICAL_REASONING_EFFORTS as readonly string[]).includes(normalized)
    ? (normalized as CanonicalReasoningEffort)
    : null;
}


/**
 * Derive one expanded fallback source's native reasoning declaration from its
 * exact provider mapping: the OpenAI effort profile for Chat/Responses and the
 * Anthropic effort-inherit profile for Messages. Only declared canonical
 * efforts become mappings, and `none` becomes an explicit disable only when the
 * mapping declares it. No control is inferred from a model name.
 */
export function fallbackModelReasoning(
  mapping: Pick<GatewayModelMapping, "reasoning_efforts"> | null | undefined,
  protocol: GatewayUpstreamProtocol,
): GatewayModelPoolReasoning | null {
  const profile =
    protocol === "anthropic_messages"
      ? "anthropic_effort_inherit"
      : "openai_effort";
  const efforts: CanonicalReasoningEffort[] = [];
  const values: string[] = [];
  for (const raw of mapping?.reasoning_efforts ?? []) {
    const trimmed = raw.trim();
    const effort = fallbackCanonicalEffort(trimmed);
    if (!effort || efforts.includes(effort)) continue;
    efforts.push(effort);
    if (effort !== "none") values.push(effort);
  }
  if (efforts.length === 0) return null;
  return {
    profile,
    values,
    supports_disable: efforts.includes("none"),
    mappings: efforts.map((effort) => ({
      effort,
      action:
        effort === "none"
          ? { kind: "disable" as const }
          : { kind: "enum" as const, value: effort },
    })),
  };
}


/**
 * Expand one stored fallback gateway model to every provider mapping that serves
 * its trimmed local model, preserving provider and mapping order. Disabled
 * providers and mappings stay included so status can report them; the runtime
 * eligibility contract is applied separately. Each source derives its reasoning
 * automatically from its exact mapping and effective protocol.
 */
export function expandModelPoolFallbackModel(
  providers: GatewayUpstreamProvider[] | null | undefined,
  localModel: string,
): GatewayModelPoolCandidate[] {
  const model = localModel.trim();
  if (!model) return [];
  const expanded: GatewayModelPoolCandidate[] = [];
  const seen = new Set<string>();
  for (const provider of providers ?? []) {
    const providerId = provider.id.trim();
    for (const mapping of provider.mappings ?? []) {
      if (mapping.local_model.trim() !== model) continue;
      const upstreamModel = mapping.upstream_model.trim();
      if (!upstreamModel) continue;
      const protocol =
        mapping.protocol ?? provider.protocol ?? "chat_completions";
      const candidate: GatewayModelPoolCandidate = {
        provider_id: providerId,
        local_model: model,
        upstream_model: upstreamModel,
        protocol,
        weight: 1,
      };
      const key = modelPoolCandidateKey(candidate);
      if (seen.has(key)) continue;
      seen.add(key);
      const reasoning = fallbackModelReasoning(mapping, protocol);
      expanded.push(reasoning ? { ...candidate, reasoning } : candidate);
    }
  }
  return expanded;
}


/**
 * Derive one pool's static reasoning catalog from the current provider list,
 * mirroring the backend `pool_reasoning_catalog`: the supported canonical
 * preferences in canonical order, the pool default and the five canonical
 * coverage rows. Readiness, cooldown, key and source-enable state never move the
 * counts; a dangling or protocol-changed reference stops being advertised.
 */
export function poolReasoningCatalog(
  providers: GatewayUpstreamProvider[] | null | undefined,
  pool: Pick<
    GatewayModelPool,
    "candidates" | "default_reasoning_effort" | "fallback_models"
  >,
): ModelPoolReasoningCatalog {
  // The configured union: regular candidates plus every provider mapping that
  // serves an ordered fallback gateway model. Static support ignores transient
  // readiness for both roles.
  const candidates = [
    ...pool.candidates,
    ...(pool.fallback_models ?? []).flatMap((model) =>
      expandModelPoolFallbackModel(providers, model),
    ),
  ];
  const total = candidates.length;
  const reasoning_coverage = CANONICAL_REASONING_EFFORTS.map((effort) => ({
    effort,
    mapped_candidates: candidates.filter((candidate) =>
      modelPoolCandidateServesEffort(providers, candidate, effort),
    ).length,
    total_candidates: total,
  }));
  return {
    reasoning_efforts: reasoning_coverage
      .filter((row) => row.mapped_candidates > 0)
      .map((row) => row.effort),
    default_reasoning_effort: resolveModelPoolDefaultEffort(pool),
    reasoning_coverage,
  };
}


/**
 * The coverage row a pool capability block surfaces: the concrete default's row
 * when one is set, otherwise the first advertised labeled preference. It never
 * silently falls back to an unlabeled canonical `low` row while a labeled tier
 * is mapped, and only falls back to the first canonical row when nothing at all
 * is advertised (so the denominator stays visible).
 */
export function selectPoolReasoningCoverageRow(
  coverage: ModelPoolReasoningCoverage[] | null | undefined,
  defaultEffort: string,
  supportedEfforts: string[],
): ModelPoolReasoningCoverage | null {
  const rows = coverage ?? [];
  if (rows.length === 0) return null;
  if (defaultEffort && defaultEffort !== "auto") {
    const exact = rows.find((row) => row.effort === defaultEffort);
    if (exact) return exact;
  }
  const advertised = rows.find((row) => supportedEfforts.includes(row.effort));
  return advertised ?? rows[0] ?? null;
}


/**
 * Automatically maps a candidate model's native reasoning levels to the
 * pool's unified canonical reasoning efforts (low, medium, high, xhigh, max, none).
 * If the candidate model declares no native levels or none match the unified
 * levels, it returns hasMatch = false and an empty mappings dictionary.
 */
export function autoBindCandidateReasoning(
  candidate: Pick<
    GatewayModelPoolCandidate,
    "provider_id" | "local_model" | "upstream_model" | "protocol" | "reasoning"
  >,
  providers: GatewayUpstreamProvider[],
): {
  mappings: Record<string, string>;
  hasMatch: boolean;
} {
  const provider = providers.find((item) => item.id === candidate.provider_id);
  const mapping = (provider?.mappings ?? []).find(
    (m) =>
      m.local_model.trim() === candidate.local_model &&
      m.upstream_model.trim() === candidate.upstream_model &&
      (m.protocol ?? provider?.protocol ?? "chat_completions") === candidate.protocol,
  );

  const rawEfforts = [
    ...(mapping?.reasoning_efforts ?? []),
    ...(candidate.reasoning?.values ?? []),
  ];
  const nativeEfforts: string[] = [];
  for (const v of rawEfforts) {
    const trimmed = v.trim();
    if (trimmed !== "" && !nativeEfforts.includes(trimmed)) {
      nativeEfforts.push(trimmed);
    }
  }

  const supportsDisable =
    nativeEfforts.some((v) => v.toLowerCase() === "none") ||
    candidate.reasoning?.supports_disable === true;

  const positiveNativeEfforts = nativeEfforts.filter(
    (v) => v.toLowerCase() !== "none",
  );

  const mappings: Record<string, string> = {};
  for (const effort of CANONICAL_REASONING_EFFORTS) {
    if (effort === "none") {
      if (supportsDisable) {
        mappings["none"] = "disable";
      }
    } else {
      const matched = positiveNativeEfforts.find(
        (v) => v.toLowerCase() === effort.toLowerCase(),
      );
      if (matched) {
        mappings[effort] = `enum:${matched}`;
      }
    }
  }

  return {
    mappings,
    hasMatch: Object.keys(mappings).length > 0,
  };
}


/**
 * Stable candidate identity key. It is not a secret; it only folds the exact
 * source tuple (provider, local/upstream model, protocol) into one string.
 */
export function modelPoolCandidateKey(
  candidate: Pick<
    GatewayModelPoolCandidate,
    "provider_id" | "local_model" | "upstream_model" | "protocol"
  >,
): string {
  return [
    candidate.provider_id,
    candidate.local_model,
    candidate.upstream_model,
    candidate.protocol,
  ].join("\u0001");
}


/** Generate a fresh opaque pool id; the backend stores it verbatim. */
export function newModelPoolId(): string {
  return `pool-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}


/** i18n keys for every nonsecret candidate availability code. */
export const MODEL_POOL_AVAILABILITY_LABEL_KEYS: Record<string, string> = {
  ready: "aiGatewayModelPoolStatusReady",
  cooling: "aiGatewayModelPoolStatusCooling",
  source_disabled: "aiGatewayModelPoolStatusSourceDisabled",
  mapping_unavailable: "aiGatewayModelPoolStatusMappingUnavailable",
  missing_provider: "aiGatewayModelPoolStatusMissingProvider",
  no_usable_key: "aiGatewayModelPoolStatusNoUsableKey",
  unsupported_protocol: "aiGatewayModelPoolStatusUnsupportedProtocol",
};


/** States derived from current intent that must never be masked by a stale snapshot. */
const HARD_CURRENT_POOL_STATES = new Set([
  "missing_provider",
  "source_disabled",
  "mapping_unavailable",
  "no_usable_key",
]);


/** A quota-marked key recovers automatically once its mark is older than 1800s. */
export const POOL_KEY_QUOTA_MARK_TTL_SECONDS = 1800;


function poolKeyUsable(key: GatewayProviderKey, nowSeconds: number): boolean {
  if (!key.enabled) return false;
  if (!key.auto_marked) return true;
  // Authentication marks are manual-only and never expire on their own.
  if (key.failure_kind === "authentication") return false;
  const markedAt = typeof key.marked_at === "number" ? key.marked_at : 0;
  if (markedAt <= 0) return false;
  return nowSeconds - markedAt > POOL_KEY_QUOTA_MARK_TTL_SECONDS;
}


/**
 * Derive a candidate's availability from the CURRENT config (provider intent,
 * exact mapping/protocol reference and key availability), optionally augmented by
 * this pool's own nonsecret status for transient signals (cooldown, live health).
 *
 * The own-pool status may add a cooling/health state over an otherwise-ready source,
 * but a stale snapshot can never turn a currently disabled/removed/mapping-mismatched
 * or key-less source back into `ready`. Another pool's status is never consulted.
 */
export function resolveModelPoolCandidateState(
  candidate: GatewayModelPoolCandidate,
  providers: GatewayUpstreamProvider[] | null | undefined,
  statusCandidate?: GatewayModelPoolCandidateStatus | null,
  nowSeconds: number = Math.floor(Date.now() / 1000),
): ResolvedModelPoolCandidateState {
  const provider =
    (providers ?? []).find((item) => item.id === candidate.provider_id) ?? null;
  const statusReason = statusCandidate?.reason?.trim()
    ? statusCandidate.reason.trim()
    : null;
  const providerName =
    provider?.name?.trim() ||
    statusCandidate?.provider_name?.trim() ||
    candidate.provider_id;

  let current = "ready";
  if (!provider) {
    current = "missing_provider";
  } else if (provider.enabled === false) {
    current = "source_disabled";
  } else {
    const mapping = (provider.mappings ?? []).find(
      (item) =>
        item.local_model.trim() === candidate.local_model &&
        item.upstream_model.trim() === candidate.upstream_model &&
        (item.protocol ?? provider.protocol ?? "chat_completions") ===
          candidate.protocol,
    );
    if (!mapping) {
      current = "mapping_unavailable";
    } else {
      const rawKeys = (provider as GatewayUpstreamProviderWithKeys).keys;
      if (Array.isArray(rawKeys)) {
        current = rawKeys.some((key) => poolKeyUsable(key, nowSeconds))
          ? "ready"
          : "no_usable_key";
      }
      // A missing/legacy `keys` field is unknown, never an automatic denial.
    }
  }

  if (HARD_CURRENT_POOL_STATES.has(current)) {
    return {
      availability: current,
      reason: statusReason,
      cooling_until: null,
      provider_name: providerName,
      ready: false,
    };
  }

  const statusAvailability = statusCandidate?.availability ?? null;
  if (statusAvailability) {
    return {
      availability: statusAvailability,
      reason: statusReason,
      cooling_until: statusCandidate?.cooling_until ?? null,
      provider_name: providerName,
      ready: statusAvailability === "ready",
    };
  }

  return {
    availability: current,
    reason: null,
    cooling_until: null,
    provider_name: providerName,
    ready: current === "ready",
  };
}
