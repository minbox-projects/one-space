/**
 * Per-instance dismissal state for AI Gateway provider-card warning pills.
 *
 * The store is front-end-owned: it persists only the opaque instance keys a user
 * explicitly dismissed (or that auto-dismissed) so a pill can hide immediately
 * without touching gateway configuration. An instance that stops being a current
 * problem is pruned on reconcile, so a later recurrence shows again.
 */

/** localStorage key holding the JSON string-array of dismissed instance keys. */
export const AI_GATEWAY_ALERT_BADGES_STORAGE_KEY = "ai-gateway-alert-badge-dismissals";

/**
 * Opaque identity for one auto-disabled mapping row:
 * `auto:providerId:localModelOrUpstream:upstreamModel`.
 */
export function autoDisabledAlertInstanceKey(
  providerId: string,
  localModel: string,
  upstreamModel: string,
): string {
  return `auto:${providerId}:${localModel.trim() || upstreamModel}:${upstreamModel}`;
}

/**
 * Opaque identity for one retired mapping row:
 * `retired:providerId:templateId:upstreamModel`.
 */
export function retiredMappingAlertInstanceKey(
  providerId: string,
  templateId: string,
  upstreamModel: string,
): string {
  return `retired:${providerId}:${templateId}:${upstreamModel}`;
}

/**
 * Resolve the storage to use. `undefined` lazily reads `window.localStorage` (so
 * callers in non-DOM environments do not crash); an explicit `null` means the
 * storage is unavailable. Access itself is guarded because reading
 * `window.localStorage` can throw in privacy-restricted contexts.
 */
function resolveStorage(storage: Storage | null | undefined): Storage | null {
  if (storage === null) return null;
  if (storage !== undefined) return storage;
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

/** Best-effort write that never throws when the storage rejects the operation. */
function persistDismissedAlertInstanceKeys(
  keys: Set<string>,
  storage?: Storage | null,
): void {
  const resolved = resolveStorage(storage);
  if (!resolved) return;
  try {
    resolved.setItem(
      AI_GATEWAY_ALERT_BADGES_STORAGE_KEY,
      JSON.stringify(Array.from(keys)),
    );
  } catch {
    // Session-only fallback: the caller still receives the in-memory set.
  }
}

/**
 * Read the persisted dismissed set. Any read, parse, or shape failure degrades to
 * an empty set and never throws.
 */
export function readDismissedAlertInstanceKeys(
  storage?: Storage | null,
): Set<string> {
  const resolved = resolveStorage(storage);
  if (!resolved) return new Set();
  let raw: string | null;
  try {
    raw = resolved.getItem(AI_GATEWAY_ALERT_BADGES_STORAGE_KEY);
  } catch {
    return new Set();
  }
  if (raw === null) return new Set();
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((value): value is string => typeof value === "string"));
  } catch {
    return new Set();
  }
}

/**
 * Merge `keys` into the persisted dismissed set and return the merged set. The
 * write is best-effort, so a failing storage still yields the session set.
 */
export function dismissAlertInstanceKeys(
  keys: Iterable<string>,
  storage?: Storage | null,
): Set<string> {
  const merged = readDismissedAlertInstanceKeys(storage);
  for (const key of keys) merged.add(key);
  persistDismissedAlertInstanceKeys(merged, storage);
  return merged;
}

/**
 * Keep only dismissed keys that are still current problem instances and persist
 * the pruned set. Returns the kept set; a key missing from `currentKeys` is
 * forgotten so a later recurrence is treated as new.
 */
export function reconcileDismissedAlertInstanceKeys(
  currentKeys: Iterable<string>,
  storage?: Storage | null,
): Set<string> {
  const current = new Set(currentKeys);
  const persisted = readDismissedAlertInstanceKeys(storage);
  const kept = new Set<string>();
  for (const key of persisted) {
    if (current.has(key)) kept.add(key);
  }
  // `kept` is always a subset of `persisted`, so an equal size means nothing was
  // pruned and the stored bytes already match: skip the no-op write.
  if (kept.size !== persisted.size) {
    persistDismissedAlertInstanceKeys(kept, storage);
  }
  return kept;
}
