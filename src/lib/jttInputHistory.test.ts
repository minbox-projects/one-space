import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  JTT_INPUT_HISTORY_KEY_PREFIX,
  addJttInputHistory,
  loadJttInputHistory,
} from "./jttInputHistory";

describe("jttInputHistory", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    localStorage.clear();
  });

  it("starts empty for every tab", () => {
    for (const tab of ["jt808", "jt809", "jt1078", "hex"] as const) {
      expect(loadJttInputHistory(tab)).toEqual([]);
    }
  });

  it("stores records newest first with ISO time and persists them", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00.000Z"));
    vi.spyOn(crypto, "randomUUID").mockReturnValueOnce("00000000-0000-4000-8000-000000000001");
    addJttInputHistory("jt808", "AAAA");

    vi.setSystemTime(new Date("2026-01-02T00:00:00.000Z"));
    vi.spyOn(crypto, "randomUUID").mockReturnValueOnce("00000000-0000-4000-8000-000000000002");
    addJttInputHistory("jt808", "BBBB");

    expect(loadJttInputHistory("jt808")).toEqual([
      { id: "00000000-0000-4000-8000-000000000002", text: "BBBB", createdAt: "2026-01-02T00:00:00.000Z" },
      { id: "00000000-0000-4000-8000-000000000001", text: "AAAA", createdAt: "2026-01-01T00:00:00.000Z" },
    ]);
    expect(
      JSON.parse(
        localStorage.getItem(`${JTT_INPUT_HISTORY_KEY_PREFIX}jt808`) ?? "[]",
      ),
    ).toHaveLength(2);
  });

  it("keeps each tab's history separate", () => {
    addJttInputHistory("jt808", "AAAA");
    addJttInputHistory("jt809", "BBBB");
    expect(loadJttInputHistory("jt808")).toHaveLength(1);
    expect(loadJttInputHistory("jt808")[0].text).toBe("AAAA");
    expect(loadJttInputHistory("jt809")[0].text).toBe("BBBB");
    expect(loadJttInputHistory("jt1078")).toEqual([]);
  });

  it("deduplicates by content and moves the repeated entry to the front with a fresh time", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00.000Z"));
    addJttInputHistory("jt808", "AAAA");
    addJttInputHistory("jt808", "BBBB");

    vi.setSystemTime(new Date("2026-01-03T00:00:00.000Z"));
    addJttInputHistory("jt808", "AAAA");

    const history = loadJttInputHistory("jt808");
    expect(history.map((item) => item.text)).toEqual(["AAAA", "BBBB"]);
    expect(history[0].createdAt).toBe("2026-01-03T00:00:00.000Z");
  });

  it("trims entries and ignores blank input", () => {
    expect(addJttInputHistory("jt808", "  AAAA  ")[0].text).toBe("AAAA");
    expect(addJttInputHistory("jt808", "   ")).toHaveLength(1);
    expect(addJttInputHistory("jt808", "")).toHaveLength(1);
  });

  it("keeps only the most recent 10 entries", () => {
    for (let index = 1; index <= 12; index += 1) {
      addJttInputHistory("jt808", `MSG-${String(index).padStart(2, "0")}`);
    }
    const history = loadJttInputHistory("jt808");
    expect(history).toHaveLength(10);
    expect(history[0].text).toBe("MSG-12");
    expect(history[9].text).toBe("MSG-03");
  });

  it("sorts stored records by time descending on load", () => {
    const records = [
      { id: "old", text: "OLD", createdAt: "2026-01-01T00:00:00.000Z" },
      { id: "new", text: "NEW", createdAt: "2026-01-03T00:00:00.000Z" },
      { id: "mid", text: "MID", createdAt: "2026-01-02T00:00:00.000Z" },
    ];
    localStorage.setItem(
      `${JTT_INPUT_HISTORY_KEY_PREFIX}jt808`,
      JSON.stringify(records),
    );
    expect(loadJttInputHistory("jt808").map((item) => item.id)).toEqual([
      "new",
      "mid",
      "old",
    ]);
  });

  it("migrates the legacy string list format into records", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-04T00:00:00.000Z"));
    vi.spyOn(crypto, "randomUUID")
      .mockReturnValueOnce("00000000-0000-4000-8000-000000000009")
      .mockReturnValueOnce("00000000-0000-4000-8000-00000000000A");
    localStorage.setItem(`${JTT_INPUT_HISTORY_KEY_PREFIX}jt808`, JSON.stringify(["LEGACY-A", "LEGACY-B"]));

    expect(loadJttInputHistory("jt808")).toEqual([
      { id: "00000000-0000-4000-8000-000000000009", text: "LEGACY-A", createdAt: "2026-01-04T00:00:00.000Z" },
      { id: "00000000-0000-4000-8000-00000000000A", text: "LEGACY-B", createdAt: "2026-01-04T00:00:00.000Z" },
    ]);
    expect(
      JSON.parse(
        localStorage.getItem(`${JTT_INPUT_HISTORY_KEY_PREFIX}jt808`) ?? "[]",
      ),
    ).toHaveLength(2);
  });

  it("recovers from corrupt stored JSON", () => {
    localStorage.setItem(`${JTT_INPUT_HISTORY_KEY_PREFIX}jt808`, "{");
    expect(loadJttInputHistory("jt808")).toEqual([]);
    expect(localStorage.getItem(`${JTT_INPUT_HISTORY_KEY_PREFIX}jt808`)).toBeNull();
  });

  it("recovers from a stored value that is not a valid record list", () => {
    localStorage.setItem(
      `${JTT_INPUT_HISTORY_KEY_PREFIX}jt808`,
      JSON.stringify([{ id: "", text: "T", createdAt: "not-a-date" }, 42]),
    );
    expect(loadJttInputHistory("jt808")).toEqual([]);
    expect(localStorage.getItem(`${JTT_INPUT_HISTORY_KEY_PREFIX}jt808`)).toBeNull();
  });
});