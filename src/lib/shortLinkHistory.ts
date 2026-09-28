import { createHistoryStore } from "@/toolbox/historyStore";
import { writeLocalJson } from "@/toolbox/localStore";

export const SHORT_LINK_HISTORY_KEY = "onespace:short-link-history";
export const SHORT_LINK_HISTORY_LIMIT = 50;

export type ShortLinkHistoryRecord = {
  id: string;
  longUrl: string;
  shortUrl: string;
  createdAt: string;
};

export type ShortLinkHistoryErrorCode = "read_failed" | "cleanup_failed" | "write_failed";

export type ShortLinkHistoryResult =
  | {
      status: "success" | "recovered";
      records: ShortLinkHistoryRecord[];
    }
  | {
      status: "failure";
      records: ShortLinkHistoryRecord[];
      error: { code: ShortLinkHistoryErrorCode };
    };

const RECORD_KEYS = ["id", "longUrl", "shortUrl", "createdAt"] as const;
const ISO_8601_DATE_TIME =
  /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(Z|[+-](\d{2}):(\d{2}))$/;

function isValidIso8601(value: string): boolean {
  const match = ISO_8601_DATE_TIME.exec(value);
  if (!match || !Number.isFinite(Date.parse(value))) return false;

  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const hour = Number(match[4]);
  const minute = Number(match[5]);
  const second = Number(match[6]);
  const offsetHour = match[8] === undefined ? 0 : Number(match[8]);
  const offsetMinute = match[9] === undefined ? 0 : Number(match[9]);
  const leapYear = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const daysInMonth = [31, leapYear ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

  return (
    month >= 1 &&
    month <= 12 &&
    day >= 1 &&
    day <= (daysInMonth[month - 1] ?? 0) &&
    hour <= 23 &&
    minute <= 59 &&
    second <= 59 &&
    offsetMinute <= 59 &&
    (offsetHour < 14 || (offsetHour === 14 && offsetMinute === 0))
  );
}

function isHistoryRecord(value: unknown): value is ShortLinkHistoryRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;

  const record = value as Record<string, unknown>;
  return (
    Object.keys(record).length === RECORD_KEYS.length &&
    RECORD_KEYS.every((key) => typeof record[key] === "string") &&
    isValidIso8601(record.createdAt as string)
  );
}

const historyStore = createHistoryStore<ShortLinkHistoryRecord>({
  storageKey: SHORT_LINK_HISTORY_KEY,
  // The store normalizes and de-duplicates; the load-time sort and the 50-entry
  // cap are applied by `newestFirst` so out-of-order records are ordered before
  // truncation. No limit here so the full valid set reaches that sort.
  limit: Number.MAX_SAFE_INTEGER,
  isValidEntry: isHistoryRecord,
});

function newestFirst(records: ShortLinkHistoryRecord[]): ShortLinkHistoryRecord[] {
  return [...records]
    .sort((left, right) => Date.parse(right.createdAt) - Date.parse(left.createdAt))
    .slice(0, SHORT_LINK_HISTORY_LIMIT);
}

function failure(
  code: ShortLinkHistoryErrorCode,
  records: ShortLinkHistoryRecord[] = [],
): ShortLinkHistoryResult {
  return { status: "failure", records, error: { code } };
}

function removeStoredHistory(): boolean {
  try {
    localStorage.removeItem(SHORT_LINK_HISTORY_KEY);
    return true;
  } catch {
    return false;
  }
}

function recoverInvalidHistory(): ShortLinkHistoryResult {
  if (!removeStoredHistory()) return failure("cleanup_failed");
  return { status: "recovered", records: [] };
}

function readStoredRaw(): { ok: true; raw: string | null } | { ok: false } {
  try {
    return { ok: true, raw: localStorage.getItem(SHORT_LINK_HISTORY_KEY) };
  } catch {
    return { ok: false };
  }
}

function persistHistory(
  records: ShortLinkHistoryRecord[],
  previousRecords: ShortLinkHistoryRecord[],
  recovered: boolean,
): ShortLinkHistoryResult {
  const normalized = newestFirst(records);
  if (!writeLocalJson(SHORT_LINK_HISTORY_KEY, normalized)) {
    return failure("write_failed", previousRecords);
  }
  return { status: recovered ? "recovered" : "success", records: normalized };
}

export function loadShortLinkHistory(): ShortLinkHistoryResult {
  const stored = readStoredRaw();
  if (!stored.ok) return failure("read_failed");
  if (stored.raw === null) return { status: "success", records: [] };

  let parsed: unknown;
  try {
    parsed = JSON.parse(stored.raw);
  } catch {
    return recoverInvalidHistory();
  }

  if (!Array.isArray(parsed) || !parsed.every(isHistoryRecord)) {
    return recoverInvalidHistory();
  }

  // The shared store performs the safe read and entry normalization; the
  // history contract additionally guarantees newest-first order and the cap.
  return { status: "success", records: newestFirst(historyStore.read()) };
}

export function addShortLinkHistory(longUrl: string, shortUrl: string): ShortLinkHistoryResult {
  const loaded = loadShortLinkHistory();
  if (loaded.status === "failure") return loaded;

  const record: ShortLinkHistoryRecord = {
    id: crypto.randomUUID(),
    longUrl,
    shortUrl,
    createdAt: new Date().toISOString(),
  };
  const records = newestFirst([record, ...loaded.records]);
  return persistHistory(records, loaded.records, loaded.status === "recovered");
}

export function deleteShortLinkHistory(id: string): ShortLinkHistoryResult {
  const loaded = loadShortLinkHistory();
  if (loaded.status === "failure") return loaded;

  const records = loaded.records.filter((record) => record.id !== id);
  if (records.length === loaded.records.length) return loaded;

  return persistHistory(records, loaded.records, loaded.status === "recovered");
}

export function clearShortLinkHistory(): ShortLinkHistoryResult {
  if (!removeStoredHistory()) return failure("write_failed");
  return { status: "success", records: [] };
}
