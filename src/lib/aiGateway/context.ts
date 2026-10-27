import type { AggregateContextResult, ContextWindowPresetItem, GatewayModelMapping, GatewayModelPool, GatewayProviderTemplate, GatewayUpstreamProtocol, GatewayUpstreamProvider } from "./types";

/** Declared context-window bounds in total tokens. */
export const CONTEXT_WINDOW_MIN = 1;

export const CONTEXT_WINDOW_MAX = 4294967295;

/** Claude's shared compaction threshold boundaries. */
export const CLAUDE_COMPACTION_MIN = 100000;

export const CLAUDE_COMPACTION_CAP = 1000000;


/** Commonly used context-window presets for operator convenience. */
export const CONTEXT_WINDOW_PRESETS = [
  "32K",
  "64K",
  "128K",
  "200K",
  "256K",
  "512K",
  "1M",
  "2M",
] as const;


export const CONTEXT_WINDOW_PRESET_ITEMS: readonly ContextWindowPresetItem[] = [
  { label: "32K", tokens: 32000 },
  { label: "64K", tokens: 64000 },
  { label: "128K", tokens: 128000 },
  { label: "200K", tokens: 200000 },
  { label: "256K", tokens: 256000 },
  { label: "512K", tokens: 512000 },
  { label: "1M", tokens: 1000000 },
  { label: "2M", tokens: 2000000 },
] as const;


/** A declared value usable as a context window; `null`/zero means unspecified. */
export function knownContextWindow(
  value: number | null | undefined,
): number | null {
  return typeof value === "number" &&
    Number.isInteger(value) &&
    value >= CONTEXT_WINDOW_MIN &&
    value <= CONTEXT_WINDOW_MAX
    ? value
    : null;
}


/**
 * Parse an operator's context-window draft. Blank means unspecified, while a
 * nonnumeric, fractional or out-of-range value stays invalid so the caller can
 * keep the draft visible and block the save.
 * Only supports positive integer tokens; compact unit strings are rejected.
 */
export function parseContextWindowInput(raw: string): {
  value: number | null;
  invalid: boolean;
} {
  const trimmed = raw.trim();
  if (trimmed === "") return { value: null, invalid: false };

  // Raw positive integer check: only support pure positive integer tokens
  if (/^\d+$/.test(trimmed)) {
    const value = Number(trimmed);
    if (
      !Number.isSafeInteger(value) ||
      value < CONTEXT_WINDOW_MIN ||
      value > CONTEXT_WINDOW_MAX
    ) {
      return { value: null, invalid: true };
    }
    return { value, invalid: false };
  }

  return { value: null, invalid: true };
}


/** Group a token count with deterministic separators for display. */
export function formatContextWindow(value: number): string {
  return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}


/**
 * Formats a context-window token count into commonly used compact units like "1M", "256K".
 * Handles both decimal (1000) and binary (1024) thresholds gracefully.
 * Returns an empty string for null, undefined, or non-positive values.
 */
export function formatContextWindowCompact(
  value: number | null | undefined,
): string {
  if (value == null || !Number.isFinite(value) || value <= 0) {
    return "";
  }

  // Megatoken boundaries (>= 1,000,000)
  if (value >= 1_000_000) {
    // Prefer clean decimal millions first: 1,000,000 -> 1M, 2,000,000 -> 2M
    if (value % 1_000_000 === 0) {
      return `${value / 1_000_000}M`;
    }
    // Binary megatokens: 1,048,576 -> 1M, 2,097,152 -> 2M
    if (value % 1_048_576 === 0) {
      return `${value / 1_048_576}M`;
    }
    // Round to 1 decimal place with proper rounding: e.g. 1,050,000 -> 1.1M, 1,500,000 -> 1.5M
    const roundedMillions = Math.round((value / 1_000_000) * 10) / 10;
    return `${roundedMillions}M`;
  }

  // Kilotoken boundaries (>= 1,000)
  if (value >= 1_000) {
    // Prefer clean decimal thousands first: 128,000 -> 128K, 256,000 -> 256K, 64,000 -> 64K
    if (value % 1000 === 0) {
      return `${value / 1000}K`;
    }
    // Binary kilotokens: 131,072 -> 128K, 65,536 -> 64K, 262,144 -> 256K, 4096 -> 4K
    if (value % 1024 === 0) {
      return `${value / 1024}K`;
    }
    // Round to 1 decimal place with proper rounding
    const roundedKilos = Math.round((value / 1000) * 10) / 10;
    if (roundedKilos >= 1000) {
      return "1M";
    }
    return `${roundedKilos}K`;
  }

  return String(value);
}


/**
 * Formats a context-window token count for standard model list and dialog displays:
 * - Below 10,000 tokens: keeps raw comma-formatted number (e.g. 4,096, 8,192).
 * - 10,000 to 999,999 tokens: compact kilotokens (e.g. 16K, 64K, 128K, 256K).
 * - 1,000,000+ tokens: compact megatokens (e.g. 1M, 1.5M, 2M).
 * Returns empty string when value is null, undefined, or non-positive.
 */
export function formatContextWindowDisplay(
  value: number | null | undefined,
): string {
  if (value == null || !Number.isFinite(value) || value <= 0) {
    return "";
  }
  if (value < 10_000) {
    return formatContextWindow(value);
  }
  return formatContextWindowCompact(value);
}


/**
 * A template model's declared context, matched by trimmed, case-sensitive
 * upstream model id. The first matching model wins, including when its value is
 * unspecified; a missing model is also unspecified.
 */
export function resolveTemplateContextWindow(
  template: GatewayProviderTemplate | null | undefined,
  upstreamModel: string,
): number | null {
  if (!template) return null;
  const target = upstreamModel.trim();
  if (target === "") return null;
  const model = template.models.find(
    (entry) => entry.upstream_model.trim() === target,
  );
  return knownContextWindow(model?.context_window);
}


function effectiveMappingProtocol(
  provider: GatewayUpstreamProvider,
  mapping: GatewayModelMapping,
): GatewayUpstreamProtocol {
  return mapping.protocol ?? provider.protocol ?? "chat_completions";
}


interface ContextSourceRef {
  providerId: string;
  localModel: string;
  upstreamModel: string;
  protocol: GatewayUpstreamProtocol;
}


/** Resolve one exact source reference's declared context; missing is unknown. */
function resolveContextSource(
  providers: GatewayUpstreamProvider[],
  ref: ContextSourceRef,
): number | null {
  const targetProviderId = ref.providerId?.trim() ?? "";
  const targetLocal = ref.localModel?.trim() ?? "";
  const targetUpstream = ref.upstreamModel?.trim() ?? "";
  if (targetProviderId === "" || targetUpstream === "") return null;

  const provider = providers.find(
    (entry) => (entry.id?.trim() ?? "") === targetProviderId,
  );
  if (!provider) return null;
  const mapping = provider.mappings.find((entry) => {
    const entryLocal = entry.local_model?.trim() ?? "";
    const entryUpstream = entry.upstream_model?.trim() ?? "";
    const localMatches = targetLocal === "" || entryLocal === targetLocal;
    return localMatches && entryUpstream === targetUpstream;
  });
  if (!mapping) return null;
  if (ref.protocol && effectiveMappingProtocol(provider, mapping) !== ref.protocol) return null;
  return knownContextWindow(mapping.context_window);
}


/**
 * Detailed aggregate analysis over deduplicated exact source identities:
 * tracks both complete conservative coverage and known declared minimums.
 */
export function analyzeSourceContext(
  providers: GatewayUpstreamProvider[],
  refs: ContextSourceRef[],
): AggregateContextResult {
  const seen = new Set<string>();
  let knownMinimum: number | null = null;
  let hasUnknown = false;
  let totalCount = 0;
  let knownCount = 0;

  for (const ref of refs) {
    const key = `${ref.providerId}\u0000${ref.localModel}\u0000${ref.upstreamModel}\u0000${ref.protocol}`;
    if (seen.has(key)) continue;
    seen.add(key);
    totalCount += 1;
    const value = resolveContextSource(providers, ref);
    if (value === null) {
      hasUnknown = true;
    } else {
      knownCount += 1;
      knownMinimum =
        knownMinimum === null ? value : Math.min(knownMinimum, value);
    }
  }

  if (totalCount === 0) {
    return {
      conservative: null,
      known: null,
      isIncomplete: true,
      totalCount: 0,
      knownCount: 0,
    };
  }

  const conservative = hasUnknown ? null : knownMinimum;
  return {
    conservative,
    known: knownMinimum,
    isIncomplete: hasUnknown,
    totalCount,
    knownCount,
  };
}


/**
 * Full summary of ordinary-model context: includes conservative effective context
 * and known conservative bound when some upstream providers lack context.
 */
export function resolveOrdinaryContextSummary(
  providers: GatewayUpstreamProvider[],
  model: string,
): AggregateContextResult {
  const publicModel = model.trim();
  if (publicModel === "") {
    return {
      conservative: null,
      known: null,
      isIncomplete: true,
      totalCount: 0,
      knownCount: 0,
    };
  }
  const refs: ContextSourceRef[] = [];
  for (const provider of providers) {
    for (const mapping of provider.mappings) {
      if (mapping.local_model.trim() !== publicModel) continue;
      refs.push({
        providerId: provider.id,
        localModel: publicModel,
        upstreamModel: mapping.upstream_model.trim(),
        protocol: effectiveMappingProtocol(provider, mapping),
      });
    }
  }
  if (refs.length === 0) {
    return {
      conservative: null,
      known: null,
      isIncomplete: true,
      totalCount: 0,
      knownCount: 0,
    };
  }
  for (const provider of providers) {
    const defaultModel = provider.default_model?.trim();
    if (!defaultModel || defaultModel !== publicModel) continue;
    const declared = provider.mappings.find(
      (mapping) => mapping.upstream_model.trim() === defaultModel,
    );
    if (!declared) {
      return {
        conservative: null,
        known: null,
        isIncomplete: true,
        totalCount: refs.length + 1,
        knownCount: 0,
      };
    }
    refs.push({
      providerId: provider.id,
      localModel: publicModel,
      upstreamModel: defaultModel,
      protocol: provider.protocol ?? "chat_completions",
    });
  }
  return analyzeSourceContext(providers, refs);
}


/**
 * Conservative ordinary-model context across every configured mapping that
 * serves `model`, plus same-provider default-model declarations. Runtime
 * readiness, keys and enabled state never raise the advertised bound.
 */
export function resolveOrdinaryContextWindow(
  providers: GatewayUpstreamProvider[],
  model: string,
): number | null {
  return resolveOrdinaryContextSummary(providers, model).conservative;
}


function expandFallbackModelRefs(
  providers: GatewayUpstreamProvider[],
  model: string,
): ContextSourceRef[] {
  const target = model.trim();
  if (target === "") return [];
  const refs: ContextSourceRef[] = [];
  for (const provider of providers) {
    for (const mapping of provider.mappings) {
      if (mapping.local_model.trim() !== target) continue;
      const upstreamModel = mapping.upstream_model.trim();
      if (upstreamModel === "") continue;
      refs.push({
        providerId: provider.id,
        localModel: target,
        upstreamModel,
        protocol: effectiveMappingProtocol(provider, mapping),
      });
    }
  }
  return refs;
}


/**
 * Full summary of pool context: includes conservative effective context
 * and known conservative bound when some upstream candidates lack context.
 */
export function resolvePoolContextSummary(
  providers: GatewayUpstreamProvider[],
  pool:
    | (Pick<GatewayModelPool, "candidates"> & {
        context_window?: number | null;
        fallback_models?: string[];
      })
    | null
    | undefined,
): AggregateContextResult {
  if (!pool) {
    return {
      conservative: null,
      known: null,
      isIncomplete: true,
      totalCount: 0,
      knownCount: 0,
    };
  }
  if (pool.context_window != null && pool.context_window > 0) {
    return {
      conservative: pool.context_window,
      known: pool.context_window,
      isIncomplete: false,
      totalCount: pool.candidates.length,
      knownCount: pool.candidates.length,
    };
  }
  const refs: ContextSourceRef[] = pool.candidates.map((candidate) => ({
    providerId: candidate.provider_id,
    localModel: candidate.local_model,
    upstreamModel: candidate.upstream_model,
    protocol: candidate.protocol,
  }));
  for (const fallback of pool.fallback_models ?? []) {
    const expanded = expandFallbackModelRefs(providers, fallback);
    if (expanded.length === 0) {
      return {
        conservative: null,
        known: null,
        isIncomplete: true,
        totalCount: refs.length + 1,
        knownCount: 0,
      };
    }
    refs.push(...expanded);
  }
  return analyzeSourceContext(providers, refs);
}


/**
 * Conservative pool context across every candidate plus expanded ordered
 * fallback model. Unresolved references make the aggregate unknown; a pool has
 * no independent editable override.
 */
export function resolvePoolContextWindow(
  providers: GatewayUpstreamProvider[],
  pool:
    | (Pick<GatewayModelPool, "candidates"> & {
        context_window?: number | null;
        fallback_models?: string[];
      })
    | null
    | undefined,
): number | null {
  return resolvePoolContextSummary(providers, pool).conservative;
}


/** Context of one exact provider mapping, used for per-source display. */
export function resolveMappingContextWindow(
  providers: GatewayUpstreamProvider[],
  providerId: string,
  localModel: string,
  upstreamModel: string,
): number | null {
  const targetProviderId = providerId?.trim() ?? "";
  const targetLocal = localModel?.trim() ?? "";
  const targetUpstream = upstreamModel?.trim() ?? "";
  if (targetProviderId === "" || targetUpstream === "") return null;

  const provider = providers.find(
    (entry) => (entry.id?.trim() ?? "") === targetProviderId,
  );
  if (!provider) return null;
  const mapping = provider.mappings.find((entry) => {
    const entryLocal = entry.local_model?.trim() ?? "";
    const entryUpstream = entry.upstream_model?.trim() ?? "";
    const localMatches = targetLocal === "" || entryLocal === targetLocal;
    return localMatches && entryUpstream === targetUpstream;
  });
  if (!mapping) return null;
  return resolveContextSource(providers, {
    providerId: targetProviderId,
    localModel: targetLocal,
    upstreamModel: targetUpstream,
    protocol: effectiveMappingProtocol(provider, mapping),
  });
}
