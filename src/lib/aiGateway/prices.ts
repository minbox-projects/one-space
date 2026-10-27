import type { GatewayPriceDraft, GatewayUpstreamProvider, ModelPrice, OffPeakPrice } from "./types";

/** Distinct non-blank mapping upstream models in first-seen order. */
export function mappedUpstreamModels(provider: GatewayUpstreamProvider): string[] {
  const models: string[] = [];
  for (const mapping of provider.mappings) {
    const upstream = mapping.upstream_model.trim();
    if (upstream !== "" && !models.includes(upstream)) models.push(upstream);
  }
  return models;
}


/** First price row owned by exactly this provider and upstream model; a global row never matches. */
export function resolveProviderPriceRow(
  prices: ModelPrice[] | null | undefined,
  providerId: string,
  upstreamModel: string,
): ModelPrice | undefined {
  return (prices ?? []).find(
    (price) =>
      price.provider_id === providerId && price.upstream_model === upstreamModel,
  );
}


/** Parse a price input, treating blank and non-finite values as 0. */
export function parsePriceNumber(value: string): number {
  const parsed = Number(value.trim());
  return Number.isFinite(parsed) ? parsed : 0;
}


/** Keep only UTC+8 weekdays (0–6), deduplicated and ascending. */
export function normalizeDraftDays(days: number[] | null | undefined): number[] {
  return Array.from(
    new Set((days ?? []).filter((day) => day >= 0 && day <= 6)),
  ).sort((a, b) => a - b);
}


/** Seed one price draft from a stored row, or a blank draft when none exists. */
export function priceRowToDraft(
  price: ModelPrice | null | undefined,
  upstreamModel: string,
  idPrefix: string,
): GatewayPriceDraft {
  if (price == null) {
    return {
      id: idPrefix,
      upstream_model: upstreamModel,
      input: "",
      cache_read: "",
      cache_write: "",
      output: "",
      enable_off_peak: false,
      off_peaks: [],
    };
  }

  const rawOffPeaks = price.off_peaks ?? [];

  const stringifyTier = (value: number | null | undefined): string =>
    value !== undefined && value !== null ? String(value) : "";

  return {
    id: idPrefix,
    upstream_model: price.upstream_model,
    input: String(price.input),
    cache_read: String(price.cache_read),
    cache_write: String(price.cache_write),
    output: String(price.output),
    enable_off_peak: rawOffPeaks.length > 0,
    off_peaks: rawOffPeaks.map((op, index) => ({
      id: `${idPrefix}-op-${index}`,
      start_time: op.start_time ?? "00:30",
      end_time: op.end_time ?? "08:30",
      input: stringifyTier(op.input),
      cache_read: stringifyTier(op.cache_read),
      cache_write: stringifyTier(op.cache_write),
      output: stringifyTier(op.output),
      days: op.days ?? [],
    })),
  };
}


/** A draft is priced when at least one standard tier holds a non-blank value. */
export function isPriceDraftPriced(draft: GatewayPriceDraft): boolean {
  return [draft.input, draft.cache_read, draft.cache_write, draft.output].some(
    (value) => value.trim() !== "",
  );
}


/** Build the stored row for a priced draft; blank drafts or blank models produce null. */
export function draftToPriceRow(draft: GatewayPriceDraft): ModelPrice | null {
  if (!isPriceDraftPriced(draft)) return null;
  const upstreamModel = draft.upstream_model.trim();
  if (upstreamModel === "") return null;

  const row: ModelPrice = {
    upstream_model: upstreamModel,
    input: parsePriceNumber(draft.input),
    cache_read: parsePriceNumber(draft.cache_read),
    cache_write: parsePriceNumber(draft.cache_write),
    output: parsePriceNumber(draft.output),
  };

  if (draft.enable_off_peak && draft.off_peaks.length > 0) {
    const offPeaks: OffPeakPrice[] = draft.off_peaks.map((op) => {
      const days = normalizeDraftDays(op.days);
      return {
        start_time: op.start_time.trim() || "00:30",
        end_time: op.end_time.trim() || "08:30",
        input: parsePriceNumber(op.input || draft.input),
        cache_read: parsePriceNumber(op.cache_read || draft.cache_read),
        cache_write: parsePriceNumber(op.cache_write || draft.cache_write),
        output: parsePriceNumber(op.output || draft.output),
        ...(days.length > 0 ? { days } : {}),
      };
    });
    row.off_peaks = offPeaks;
  }

  return row;
}
