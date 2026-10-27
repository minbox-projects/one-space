import { normalizeReasoningEfforts } from "./providers";
import type { AggregatedModel, AggregatedModelProvider, GatewayUpstreamProvider } from "./types";

/**
 * Aggregate the local models served by enabled providers.
 *
 * A provider contributes every non-blank mapping that is not user-disabled and
 * not auto-disabled. Results are grouped by local model and sorted
 * deterministically. Default models are fallback targets for unmapped requests
 * and are not listed here.
 */
export function aggregateModels(
  providers: GatewayUpstreamProvider[],
): AggregatedModel[] {
  const groups = new Map<string, AggregatedModelProvider[]>();

  providers.forEach((provider) => {
    if (!provider.enabled) return;

    provider.mappings.forEach((mapping) => {
      if (mapping.enabled === false || mapping.auto_disabled) return;
      const localModel = mapping.local_model.trim();
      if (!localModel) return;
      const upstreamModel = mapping.upstream_model.trim();
      if (!upstreamModel) return;
      const entries = groups.get(localModel) ?? [];
      const displayName = mapping.display_name?.trim();
      const reasoningEfforts = normalizeReasoningEfforts(mapping.reasoning_efforts);
      entries.push({
        providerId: provider.id,
        providerName: provider.name,
        upstreamModel,
        endpoint: mapping.protocol ?? provider.protocol ?? "chat_completions",
        isDefault: false,
        ...(displayName ? { displayName } : {}),
        ...(reasoningEfforts.length > 0 ? { reasoningEfforts } : {}),
      });
      groups.set(localModel, entries);
    });
  });

  return Array.from(groups.entries())
    .map(([model, entries]) => {
      const sortedProviders = [...entries].sort((a, b) => {
        const byName = a.providerName.localeCompare(b.providerName);
        return byName !== 0 ? byName : a.upstreamModel.localeCompare(b.upstreamModel);
      });
      const unifiedEfforts = normalizeReasoningEfforts(
        sortedProviders.flatMap((p) => p.reasoningEfforts ?? []),
      );
      return {
        model,
        providers: sortedProviders,
        ...(unifiedEfforts.length > 0 ? { reasoningEfforts: unifiedEfforts } : {}),
      };
    })
    .sort((a, b) => a.model.localeCompare(b.model));
}


/**
 * Resolve the display name for an aggregated local model.
 *
 * The first mapping source declaring a non-empty `displayName` wins; otherwise
 * the name falls back to the first mapping source's `upstreamModel`, and finally
 * to the local model itself when there are no sources.
 */
export function resolveAggregatedModelName(entry: AggregatedModel): string {
  for (const provider of entry.providers) {
    const displayName = provider.displayName?.trim();
    if (displayName) return displayName;
  }
  const source = entry.providers[0];
  return source ? source.upstreamModel.trim() : entry.model;
}


/**
 * Resolve the aggregated list of reasoning efforts supported by an aggregated model.
 *
 * Returns the unified list from all enabled sources; falls back to an empty list.
 */
export function resolveAggregatedReasoningEfforts(entry: AggregatedModel): string[] {
  if (entry.reasoningEfforts && entry.reasoningEfforts.length > 0) {
    return entry.reasoningEfforts;
  }
  return normalizeReasoningEfforts(
    entry.providers.flatMap((p) => p.reasoningEfforts ?? []),
  );
}
