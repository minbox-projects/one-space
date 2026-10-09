import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { writeLocalJson } from "@/toolbox/localStore";

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
