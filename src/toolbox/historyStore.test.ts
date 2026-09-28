import { beforeEach, describe, expect, it } from "vitest";
import { createHistoryStore } from "@/toolbox/historyStore";
import type { HistoryStoreOptions } from "@/toolbox/historyStore";

type Entry = { id: string; value: string };

const KEY = "test:history-store";

function isEntry(value: unknown): value is Entry {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  return typeof record.id === "string" && typeof record.value === "string";
}

function makeStore(
  overrides: Partial<HistoryStoreOptions<Entry>> = {},
) {
  return createHistoryStore<Entry>({
    storageKey: KEY,
    limit: 10,
    isValidEntry: isEntry,
    ...overrides,
  });
}

function entry(id: string, value = id): Entry {
  return { id, value };
}

function storedRaw(): string | null {
  return localStorage.getItem(KEY);
}

describe("createHistoryStore read", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("returns an empty list when nothing is stored", () => {
    expect(makeStore().read()).toEqual([]);
  });

  it("returns persisted entries in stored (newest-first) order", () => {
    const store = makeStore();
    const newest = entry("newest", "n");
    const older = entry("older", "o");
    localStorage.setItem(KEY, JSON.stringify([newest, older]));

    expect(store.read()).toEqual([newest, older]);
  });

  it("drops entries that fail isValidEntry", () => {
    const store = makeStore();
    localStorage.setItem(
      KEY,
      JSON.stringify([entry("valid"), { id: "missing-value" }, 42, null]),
    );

    expect(store.read()).toEqual([entry("valid")]);
  });

  it.each([
    ["corrupt JSON", "{ not json"],
    ["a non-array record", JSON.stringify({ id: "not-an-array" })],
  ])("clears %s and returns an empty list", (_label, raw) => {
    const store = makeStore();
    localStorage.setItem(KEY, raw);

    expect(store.read()).toEqual([]);
    expect(storedRaw()).toBeNull();
  });
});

describe("createHistoryStore add", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("prepends new entries and persists them", () => {
    const store = makeStore();

    expect(store.add(entry("a"))).toEqual([entry("a")]);
    expect(store.add(entry("b"))).toEqual([entry("b"), entry("a")]);
    expect(store.read()).toEqual([entry("b"), entry("a")]);
  });

  it("de-duplicates by JSON identity by default", () => {
    const store = makeStore();

    store.add(entry("a", "same"));
    store.add({ id: "a", value: "same" });
    store.add(entry("b"));

    expect(store.read()).toEqual([entry("b"), entry("a", "same")]);
  });

  it("honours a custom identity function", () => {
    const store = makeStore({ identity: (candidate) => candidate.id });

    store.add(entry("a", "first"));
    const result = store.add(entry("a", "second"));

    expect(result).toEqual([entry("a", "second")]);
    expect(store.read()).toEqual([entry("a", "second")]);
  });

  it("caps the list at the configured limit, keeping the newest entries", () => {
    const store = makeStore({ limit: 2 });

    store.add(entry("a"));
    store.add(entry("b"));
    const result = store.add(entry("c"));

    expect(result).toEqual([entry("c"), entry("b")]);
    expect(store.read()).toEqual([entry("c"), entry("b")]);
  });

  it("treats a stored corrupt record as empty when adding", () => {
    const store = makeStore();
    localStorage.setItem(KEY, "{ broken");

    expect(store.add(entry("a"))).toEqual([entry("a")]);
    expect(store.read()).toEqual([entry("a")]);
  });
});

describe("createHistoryStore write", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("drops invalid entries on write", () => {
    const store = makeStore();

    store.write([
      entry("a"),
      { id: "missing-value" } as unknown as Entry,
      entry("b"),
    ]);

    expect(store.read()).toEqual([entry("a"), entry("b")]);
  });

  it("de-duplicates written entries by identity", () => {
    const store = makeStore({ identity: (candidate) => candidate.id });

    store.write([entry("a", "first"), entry("a", "second"), entry("b")]);

    const read = store.read();
    expect(read.map((candidate) => candidate.id)).toEqual(
      expect.arrayContaining(["a", "b"]),
    );
    expect(read.filter((candidate) => candidate.id === "a")).toHaveLength(1);
  });

  it("caps the written list at the configured limit", () => {
    const store = makeStore({ limit: 2 });

    store.write([entry("a"), entry("b"), entry("c"), entry("d")]);

    const read = store.read();
    expect(read).toHaveLength(2);
    expect(new Set(read.map((candidate) => candidate.id)).size).toBe(2);
  });

  it("persists the normalized list and clears on clear()", () => {
    const store = makeStore();
    store.write([entry("a"), entry("b")]);
    expect(store.read()).toEqual([entry("a"), entry("b")]);

    store.clear();
    expect(store.read()).toEqual([]);
    expect(storedRaw()).toBeNull();
  });
});
