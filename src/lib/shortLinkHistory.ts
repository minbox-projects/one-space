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
    offsetHour <= 23 &&
    offsetMinute <= 59
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

function recoverInvalidHistory(): ShortLinkHistoryResult {
  try {
    localStorage.removeItem(SHORT_LINK_HISTORY_KEY);
    return { status: "recovered", records: [] };
  } catch {
    return failure("cleanup_failed");
  }
}

function persistHistory(
  records: ShortLinkHistoryRecord[],
  previousRecords: ShortLinkHistoryRecord[],
  recovered: boolean,
): ShortLinkHistoryResult {
  try {
    localStorage.setItem(SHORT_LINK_HISTORY_KEY, JSON.stringify(records));
    return { status: recovered ? "recovered" : "success", records };
  } catch {
    return failure("write_failed", previousRecords);
  }
}

export function loadShortLinkHistory(): ShortLinkHistoryResult {
  let stored: string | null;

  try {
    stored = localStorage.getItem(SHORT_LINK_HISTORY_KEY);
  } catch {
    return failure("read_failed");
  }

  if (stored === null) return { status: "success", records: [] };

  let parsed: unknown;
  try {
    parsed = JSON.parse(stored);
  } catch {
    return recoverInvalidHistory();
  }

  if (!Array.isArray(parsed) || !parsed.every(isHistoryRecord)) {
    return recoverInvalidHistory();
  }

  return { status: "success", records: newestFirst(parsed) };
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
  try {
    localStorage.removeItem(SHORT_LINK_HISTORY_KEY);
    return { status: "success", records: [] };
  } catch {
    return failure("write_failed");
  }
}
