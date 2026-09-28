import { writeLocalJson } from "./localStore";

export type HistoryStoreOptions<T> = {
  storageKey: string;
  limit: number;
  isValidEntry: (value: unknown) => value is T;
  identity?: (entry: T) => string;
};

export type HistoryStore<T> = {
  read(): T[];
  write(entries: readonly T[]): void;
  add(entry: T): T[];
  clear(): void;
};

export function createHistoryStore<T>(
  options: HistoryStoreOptions<T>,
): HistoryStore<T> {
  const { storageKey, limit, isValidEntry } = options;
  const identity = options.identity ?? ((entry: T) => JSON.stringify(entry));

  const normalize = (entries: readonly unknown[]): T[] => {
    const result: T[] = [];
    const seen = new Set<string>();
    for (const candidate of entries) {
      if (!isValidEntry(candidate)) continue;
      const key = identity(candidate);
      if (seen.has(key)) continue;
      seen.add(key);
      result.push(candidate);
      if (result.length >= limit) break;
    }
    return result;
  };

  const clear = (): void => {
    try {
      localStorage.removeItem(storageKey);
    } catch {
      // Storage access can be denied; there is nothing else to do.
    }
  };

  const read = (): T[] => {
    let raw: string | null;
    try {
      raw = localStorage.getItem(storageKey);
    } catch {
      return [];
    }
    if (raw === null) return [];

    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      clear();
      return [];
    }

    if (!Array.isArray(parsed)) {
      clear();
      return [];
    }

    return normalize(parsed);
  };

  const write = (entries: readonly T[]): void => {
    writeLocalJson(storageKey, normalize(entries));
  };

  const add = (entry: T): T[] => {
    const next = normalize([entry, ...read()]);
    write(next);
    return next;
  };

  return { read, write, add, clear };
}
