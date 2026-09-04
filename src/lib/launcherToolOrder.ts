export const LAUNCHER_INTERNAL_TOOLS_ORDER_KEY =
  "onespace_launcher_internal_tools_order";
export const MORE_TOOLS_ORDER_KEY = "onespace_more_tools_order";

export function moveItemInList<T>(
  items: readonly T[],
  fromIndex: number,
  toIndex: number,
): T[] {
  if (
    fromIndex < 0 ||
    toIndex < 0 ||
    fromIndex >= items.length ||
    toIndex >= items.length ||
    fromIndex === toIndex
  ) {
    return [...items];
  }
  const next = [...items];
  const [moved] = next.splice(fromIndex, 1);
  next.splice(toIndex, 0, moved);
  return next;
}

export function applySavedOrder<T extends { id: string }>(
  items: readonly T[],
  savedOrder: string[],
): T[] {
  const known = new Set(items.map((item) => item.id));
  const order = savedOrder.filter((id) => known.has(id));
  const byId = new Map(items.map((item) => [item.id, item]));
  const ordered = order
    .map((id) => byId.get(id))
    .filter((item): item is T => item !== undefined);
  const remaining = items.filter((item) => !order.includes(item.id));
  return [...ordered, ...remaining];
}

export function readSavedOrder(storageKey: string): string[] {
  try {
    const raw = localStorage.getItem(storageKey);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((id): id is string => typeof id === "string");
  } catch {
    return [];
  }
}

export function writeSavedOrder(storageKey: string, ids: string[]): void {
  localStorage.setItem(storageKey, JSON.stringify(ids));
}