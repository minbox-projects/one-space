import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readLocalJson, writeLocalJson } from "@/toolbox/localStore";

describe("readLocalJson", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("returns the parsed value for valid JSON", () => {
    localStorage.setItem("key", JSON.stringify({ value: 7, tags: ["a"] }));
    expect(readLocalJson("key", { value: 0, tags: [] as string[] })).toEqual({
      value: 7,
      tags: ["a"],
    });
  });

  it("returns the fallback for a missing key", () => {
    const fallback = { value: 1 };
    expect(readLocalJson("missing-key", fallback)).toEqual(fallback);
  });

  it("returns the fallback for corrupt JSON", () => {
    localStorage.setItem("key", "{ not valid json");
    const fallback = { value: 1 };
    expect(readLocalJson("key", fallback)).toEqual(fallback);
  });

  it("returns the fallback when localStorage.getItem throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("storage denied");
    });
    const fallback = { value: 1 };
    expect(readLocalJson("key", fallback)).toEqual(fallback);
  });

  it("never throws on unexpected localStorage access errors", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError");
    });
    expect(() => readLocalJson("key", null)).not.toThrow();
    expect(readLocalJson("key", null)).toBeNull();
  });
});

describe("writeLocalJson", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("stores JSON and returns true on success", () => {
    expect(writeLocalJson("key", { value: 3, list: [1, 2] })).toBe(true);
    expect(JSON.parse(localStorage.getItem("key") ?? "null")).toEqual({
      value: 3,
      list: [1, 2],
    });
  });

  it("returns false without throwing when setItem fails", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("quota exceeded", "QuotaExceededError");
    });

    expect(() => writeLocalJson("key", { value: 1 })).not.toThrow();
    expect(writeLocalJson("key", { value: 1 })).toBe(false);
  });
});
