import {
  CANONICAL_REASONING_EFFORTS,
  modelPoolCandidateKey,
  type GatewayModelPoolCandidate,
  type GatewayModelPoolReasoning,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
  type ModelPoolReasoningAction,
} from "@/lib/aiGateway";

export type DraftCandidate = GatewayModelPoolCandidate & { key: string };

/**
 * The candidate-local reasoning draft. It stores only the operator's chosen
 * bindings — public effort -> stable mapping option — and never an editable
 * profile, declared value, budget range, disable toggle or budget token. The
 * immutable stored `candidate.reasoning` remains the read-only base the payload
 * is derived from, so no manual-parameter state ever goes dormant here.
 */
export type ReasoningDraft = {
  /** public effort -> mapping option (`""` unmapped, `enum:<v>`, `budget:<tokens>`, `disable`). */
  mappings: Record<string, string>;
};

/** Convert one stored typed action into its stable binding option value. */
function actionToOptionValue(action: ModelPoolReasoningAction): string {
  if (action?.kind === "enum" && typeof action.value === "string") {
    return `enum:${action.value}`;
  }
  if (action?.kind === "budget") {
    return typeof action.budget_tokens === "number"
      ? `budget:${action.budget_tokens}`
      : "budget";
  }
  if (action?.kind === "disable") return "disable";
  return "";
}

/** Seed a draft from the stored declaration's bindings only. */
export function reasoningDraftFrom(
  reasoning: GatewayModelPoolReasoning | null | undefined,
): ReasoningDraft {
  const mappings: Record<string, string> = {};
  for (const mapping of reasoning?.mappings ?? []) {
    const option = actionToOptionValue(mapping.action);
    if (option !== "") mappings[mapping.effort] = option;
  }
  return { mappings };
}

/** The bounded adapter a candidate protocol uses for its first explicit binding. */
function defaultProfileForProtocol(protocol: GatewayUpstreamProtocol): string {
  return protocol === "anthropic_messages"
    ? "anthropic_effort_inherit"
    : "openai_effort";
}

/** The reserved disable/omission native token; never a positive binding target. */
export function isReservedNoneEffort(value: string): boolean {
  return value.trim().toLowerCase() === "none";
}

/** The exact source mapping one candidate references, if it currently resolves. */
function findCandidateMapping(
  providers: GatewayUpstreamProvider[],
  candidate: Pick<
    GatewayModelPoolCandidate,
    "provider_id" | "local_model" | "upstream_model" | "protocol"
  >,
) {
  const provider = providers.find((item) => item.id === candidate.provider_id);
  if (!provider) return null;
  return (
    (provider.mappings ?? []).find(
      (mapping) =>
        mapping.local_model.trim() === candidate.local_model &&
        mapping.upstream_model.trim() === candidate.upstream_model &&
        (mapping.protocol ?? provider.protocol ?? "chat_completions") ===
          candidate.protocol,
    ) ?? null
  );
}

/**
 * The native reasoning levels offered for one candidate: the exact source
 * mapping's declared `reasoning_efforts`, unioned with the candidate's stored
 * declared values, its stored enum actions and its currently bound enums.
 * Everything is trimmed and de-duplicated without rewriting its case, and no
 * sibling model, other provider or name-based profile guess contributes.
 */
export function candidateNativeEfforts(
  providers: GatewayUpstreamProvider[],
  candidate: DraftCandidate,
  draft: ReasoningDraft,
): string[] {
  const mapping = findCandidateMapping(providers, candidate);
  const storedEnum = (candidate.reasoning?.mappings ?? [])
    .filter(
      (item) => item.action?.kind === "enum" && typeof item.action.value === "string",
    )
    .map((item) => item.action.value as string);
  const boundEnum = Object.values(draft.mappings)
    .filter((option) => option.startsWith("enum:"))
    .map((option) => option.slice("enum:".length));
  const normalized: string[] = [];
  for (const value of [
    ...(mapping?.reasoning_efforts ?? []),
    ...(candidate.reasoning?.values ?? []),
    ...storedEnum,
    ...boundEnum,
  ]) {
    const trimmed = value.trim();
    if (trimmed !== "" && !normalized.includes(trimmed)) normalized.push(trimmed);
  }
  return normalized;
}

/** Convert one binding option back into its typed native action. */
export function optionValueToAction(
  option: string,
): ModelPoolReasoningAction | null {
  if (option.startsWith("enum:")) {
    return { kind: "enum", value: option.slice("enum:".length) };
  }
  if (option.startsWith("budget:")) {
    const tokens = Number(option.slice("budget:".length).trim());
    return Number.isInteger(tokens)
      ? { kind: "budget", budget_tokens: tokens }
      : { kind: "budget" };
  }
  if (option === "budget") return { kind: "budget" };
  if (option === "disable") return { kind: "disable" };
  return null;
}

/** The distinct native budget tokens a stored declaration offers as rebind targets. */
export function storedBudgetTokens(
  reasoning: GatewayModelPoolReasoning | null | undefined,
): number[] {
  const tokens: number[] = [];
  for (const mapping of reasoning?.mappings ?? []) {
    const value = mapping.action?.budget_tokens;
    if (mapping.action?.kind === "budget" && typeof value === "number") {
      if (!tokens.includes(value)) tokens.push(value);
    }
  }
  return tokens;
}

/** Whether a stored declaration carries any metadata at all. */
function hasStoredReasoning(
  reasoning: GatewayModelPoolReasoning | null | undefined,
): boolean {
  if (!reasoning) return false;
  return (
    (reasoning.profile ?? "") !== "" ||
    (reasoning.values ?? []).length > 0 ||
    reasoning.budget != null ||
    reasoning.supports_disable === true ||
    (reasoning.mappings ?? []).length > 0
  );
}

/**
 * Derive a candidate's persisted reasoning payload from its immutable stored
 * base and the operator's chosen bindings. The stored `profile`, `values`,
 * `budget` and `supports_disable` are retained as read-only inputs, so editing
 * never rewrites them; the payload only appends a chosen native enum value to
 * `values` when needed and sets `supports_disable` when the operator binds `none`
 * to an explicit disable. An untouched new candidate (no stored declaration and
 * no binding) emits nothing, so clearing every binding never leaves an orphaned
 * declaration.
 */
export function deriveReasoning(
  base: GatewayModelPoolReasoning | null | undefined,
  draft: ReasoningDraft,
  protocol: GatewayUpstreamProtocol,
): GatewayModelPoolReasoning | undefined {
  const bindings = CANONICAL_REASONING_EFFORTS.flatMap((effort) => {
    const action = optionValueToAction(draft.mappings[effort] ?? "");
    return action ? [{ effort, action }] : [];
  });
  if (!hasStoredReasoning(base) && bindings.length === 0) return undefined;

  const values = [...(base?.values ?? [])];
  for (const binding of bindings) {
    if (binding.action.kind !== "enum" || typeof binding.action.value !== "string") {
      continue;
    }
    if (!values.includes(binding.action.value)) values.push(binding.action.value);
  }
  const storedProfile = base?.profile ?? "";
  return {
    profile:
      storedProfile !== "" ? storedProfile : defaultProfileForProtocol(protocol),
    values,
    ...(base?.budget != null ? { budget: base.budget } : {}),
    supports_disable:
      base?.supports_disable === true ||
      bindings.some((binding) => binding.action.kind === "disable"),
    mappings: bindings,
  };
}

/** Stable full-identity declaration signature for one effort, order-independent. */
export function effortDeclarationSignature(
  rows: Array<{ key: string; action: ModelPoolReasoningAction | null }>,
): string {
  const entries = rows
    .filter(
      (row): row is { key: string; action: ModelPoolReasoningAction } =>
        row.action != null,
    )
    .map((row) => [row.key, JSON.stringify(row.action)] as const)
    .sort((a, b) => a[0].localeCompare(b[0]));
  return JSON.stringify(entries);
}

export function toDraft(candidate: GatewayModelPoolCandidate): DraftCandidate {
  return { ...candidate, key: modelPoolCandidateKey(candidate) };
}

export function parseWeight(value: string): number | null {
  const trimmed = value.trim();
  if (trimmed === "") return null;
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed < 1 || parsed > 100) return null;
  return parsed;
}
