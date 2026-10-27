export interface HistoryEntry {
  timestamp: number;
  ts?: number;
  content?: string;
  snapshot?: any;
  action?: string;
  summary?: string;
}

export interface DiffRow {
  key: string;
  kind: 'added' | 'removed' | 'changed';
  before: string;
  after: string;
  beforeValue: any;
  afterValue: any;
}

export function historyTimestamp(entry: HistoryEntry) {
  if (typeof entry.timestamp === 'number') return entry.timestamp;
  if (typeof entry.ts === 'number') return entry.ts * 1000;
  return 0;
}

export function formatDiffValue(value: any) {
  if (value === undefined) return 'Not set';
  if (typeof value === 'string') return value || 'Not set';
  if (value === null) return 'Not set';
  return JSON.stringify(value);
}

export function diffValueIsSet(value: any) {
  return value !== undefined && value !== null && value !== '';
}

export function copyableDiffValue(value: any) {
  if (!diffValueIsSet(value)) return '';
  if (typeof value === 'string') return value;
  return JSON.stringify(value, null, 2);
}

export function flattenDiffFields(value: any, prefix = '', out: Record<string, any> = {}) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    if (prefix) out[prefix] = value;
    return out;
  }
  Object.entries(value).forEach(([key, nested]) => {
    if (key === 'history') return;
    const path = prefix ? `${prefix}.${key}` : key;
    if (nested && typeof nested === 'object' && !Array.isArray(nested)) {
      flattenDiffFields(nested, path, out);
    } else {
      out[path] = nested;
    }
  });
  return out;
}

export function buildHistoryDiff(current: any, entry: HistoryEntry): DiffRow[] {
  const snapshot = entry.snapshot;
  if (!snapshot || typeof snapshot !== 'object') return [];
  const before = flattenDiffFields(snapshot);
  const after = flattenDiffFields(current || {});
  const keys = Array.from(new Set([...Object.keys(before), ...Object.keys(after)]))
    .filter((key) => key !== 'history')
    .sort();

  return keys.flatMap((key) => {
    const beforeValue = before[key];
    const afterValue = after[key];
    if (JSON.stringify(beforeValue) === JSON.stringify(afterValue)) return [];
    const kind = beforeValue === undefined ? 'added' : afterValue === undefined ? 'removed' : 'changed';
    return [{
      key,
      kind,
      before: formatDiffValue(beforeValue),
      after: formatDiffValue(afterValue),
      beforeValue,
      afterValue,
    }];
  });
}
