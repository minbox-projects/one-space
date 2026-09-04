export const JTT_INPUT_HISTORY_LIMIT = 10;
export const JTT_INPUT_HISTORY_KEY_PREFIX = "onespace:jtt-input-history:";

export type JttInputHistoryTab = "jt808" | "jt809" | "jt1078" | "hex";

export type JttInputHistoryRecord = {
  id: string;
  text: string;
  createdAt: string;
};

function historyKey(tab: JttInputHistoryTab): string {
  return `${JTT_INPUT_HISTORY_KEY_PREFIX}${tab}`;
}

function isHistoryRecord(value: unknown): value is JttInputHistoryRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;

  const record = value as Record<string, unknown>;
  return (
    typeof record.id === "string" &&
    record.id.length > 0 &&
    typeof record.text === "string" &&
    record.text.trim().length > 0 &&
    typeof record.createdAt === "string" &&
    Number.isFinite(Date.parse(record.createdAt))
  );
}

function isLegacyEntry(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function newestFirst(records: JttInputHistoryRecord[]): JttInputHistoryRecord[] {
  const seen = new Set<string>();
  const unique: JttInputHistoryRecord[] = [];
  for (const record of [...records].sort(
    (left, right) => Date.parse(right.createdAt) - Date.parse(left.createdAt),
  )) {
    const text = record.text.trim();
    if (seen.has(text)) continue;
    seen.add(text);
    unique.push({ ...record, text });
  }
  return unique.slice(0, JTT_INPUT_HISTORY_LIMIT);
}

function persistHistory(tab: JttInputHistoryTab, records: JttInputHistoryRecord[]): void {
  try {
    localStorage.setItem(historyKey(tab), JSON.stringify(records));
  } catch {
    // best effort; the session list is still returned to the caller
  }
}

function recoverInvalidHistory(tab: JttInputHistoryTab): JttInputHistoryRecord[] {
  try {
    localStorage.removeItem(historyKey(tab));
  } catch {
    // best effort; a failed cleanup still returns an empty session list
  }
  return [];
}

export function loadJttInputHistory(tab: JttInputHistoryTab): JttInputHistoryRecord[] {
  let stored: string | null;
  try {
    stored = localStorage.getItem(historyKey(tab));
  } catch {
    return [];
  }
  if (stored === null) return [];

  let parsed: unknown;
  try {
    parsed = JSON.parse(stored);
  } catch {
    return recoverInvalidHistory(tab);
  }

  if (!Array.isArray(parsed)) return recoverInvalidHistory(tab);

  if (parsed.every(isHistoryRecord)) {
    return newestFirst(parsed);
  }

  if (parsed.every(isLegacyEntry)) {
    const migrated = newestFirst(
      parsed.map((text) => ({
        id: crypto.randomUUID(),
        text,
        createdAt: new Date().toISOString(),
      })),
    );
    persistHistory(tab, migrated);
    return migrated;
  }

  return recoverInvalidHistory(tab);
}

export function addJttInputHistory(tab: JttInputHistoryTab, text: string): JttInputHistoryRecord[] {
  const entry = text.trim();
  if (entry === "") return loadJttInputHistory(tab);

  const current = loadJttInputHistory(tab);
  const record: JttInputHistoryRecord = {
    id: crypto.randomUUID(),
    text: entry,
    createdAt: new Date().toISOString(),
  };
  const updated = newestFirst([record, ...current]);
  persistHistory(tab, updated);
  return updated;
}