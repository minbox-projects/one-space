import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  SHORT_LINK_HISTORY_KEY,
  addShortLinkHistory,
  clearShortLinkHistory,
  deleteShortLinkHistory,
  loadShortLinkHistory,
  type ShortLinkHistoryRecord,
} from "@/lib/shortLinkHistory";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

function record(index: number, createdAt: string): ShortLinkHistoryRecord {
  return {
    id: `id-${index}`,
    longUrl: `https://example.com/long/${index}`,
    shortUrl: `https://tinyurl.com/test-${index}`,
    createdAt,
  };
}

describe("short link history", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    localStorage.clear();
    resetTauriMocks();
  });

  it("用 randomUUID 和 ISO 时间新增记录，重载后最新在前且不去重", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00.000Z"));
    vi.spyOn(crypto, "randomUUID").mockReturnValueOnce("00000000-0000-4000-8000-000000000001");

    expect(addShortLinkHistory("https://example.com/same", "https://tinyurl.com/same")).toMatchObject({
      status: "success",
      records: [{
        id: "00000000-0000-4000-8000-000000000001",
        createdAt: "2026-01-01T00:00:00.000Z",
      }],
    });

    vi.setSystemTime(new Date("2026-01-02T00:00:00.000Z"));
    vi.spyOn(crypto, "randomUUID").mockReturnValueOnce("00000000-0000-4000-8000-000000000002");
    const second = addShortLinkHistory("https://example.com/same", "https://tinyurl.com/same");

    expect(second.status).toBe("success");
    expect(second.records).toHaveLength(2);
    expect(second.records.map((item) => item.id)).toEqual([
      "00000000-0000-4000-8000-000000000002",
      "00000000-0000-4000-8000-000000000001",
    ]);
    expect(loadShortLinkHistory()).toEqual(second);
  });

  it("加载时按真实时间倒序并只保留最新 50 条", () => {
    const records = Array.from({ length: 52 }, (_, index) =>
      record(index, new Date(Date.UTC(2026, 0, index + 1)).toISOString()),
    ).reverse();
    localStorage.setItem(SHORT_LINK_HISTORY_KEY, JSON.stringify(records));

    const result = loadShortLinkHistory();

    expect(result.status).toBe("success");
    expect(result.records).toHaveLength(50);
    expect(result.records[0]?.id).toBe("id-51");
    expect(result.records.at(-1)?.id).toBe("id-2");
  });

  it.each([
    ["损坏 JSON", "{"],
    ["非数组", JSON.stringify({})],
    ["缺少字段", JSON.stringify([{ id: "id", longUrl: "https://example.com" }])],
    [
      "额外字段",
      JSON.stringify([{ ...record(1, "2026-01-01T00:00:00.000Z"), migrated: true }]),
    ],
    ["无效 ISO 日期", JSON.stringify([record(1, "2026-02-30T00:00:00.000Z")])],
    ["任一记录损坏", JSON.stringify([record(1, "2026-01-01T00:00:00.000Z"), null])],
  ])("%s 时丢弃整个 key，只返回一次恢复状态", (_label, stored) => {
    localStorage.setItem(SHORT_LINK_HISTORY_KEY, stored);

    expect(loadShortLinkHistory()).toEqual({ status: "recovered", records: [] });
    expect(localStorage.getItem(SHORT_LINK_HISTORY_KEY)).toBeNull();
    expect(loadShortLinkHistory()).toEqual({ status: "success", records: [] });
  });

  it("损坏数据清理后可从空历史继续新增", () => {
    localStorage.setItem(SHORT_LINK_HISTORY_KEY, "{");
    vi.spyOn(crypto, "randomUUID").mockReturnValue("00000000-0000-4000-8000-000000000003");

    const result = addShortLinkHistory("https://example.com/new", "https://tinyurl.com/new");

    expect(result.status).toBe("recovered");
    expect(result.records).toHaveLength(1);
    expect(loadShortLinkHistory()).toMatchObject({ status: "success", records: result.records });
  });

  it("区分空历史、读取被拒绝、损坏清理失败和配额写入失败", () => {
    expect(loadShortLinkHistory()).toEqual({ status: "success", records: [] });

    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("denied", "SecurityError");
    });
    expect(loadShortLinkHistory()).toEqual({
      status: "failure",
      records: [],
      error: { code: "read_failed" },
    });
    vi.restoreAllMocks();

    localStorage.setItem(SHORT_LINK_HISTORY_KEY, "{");
    vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new DOMException("denied", "SecurityError");
    });
    expect(loadShortLinkHistory()).toEqual({
      status: "failure",
      records: [],
      error: { code: "cleanup_failed" },
    });
    vi.restoreAllMocks();

    localStorage.clear();
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("quota", "QuotaExceededError");
    });
    expect(addShortLinkHistory("https://example.com", "https://tinyurl.com/test")).toEqual({
      status: "failure",
      records: [],
      error: { code: "write_failed" },
    });
  });

  it("删除单条和清空只修改历史 key，不调用 Tauri", () => {
    localStorage.setItem("unrelated", "keep");
    localStorage.setItem(
      SHORT_LINK_HISTORY_KEY,
      JSON.stringify([
        record(2, "2026-01-02T00:00:00.000Z"),
        record(1, "2026-01-01T00:00:00.000Z"),
      ]),
    );

    expect(deleteShortLinkHistory("id-2")).toEqual({
      status: "success",
      records: [record(1, "2026-01-01T00:00:00.000Z")],
    });
    expect(localStorage.getItem("unrelated")).toBe("keep");
    expect(clearShortLinkHistory()).toEqual({ status: "success", records: [] });
    expect(localStorage.getItem(SHORT_LINK_HISTORY_KEY)).toBeNull();
    expect(localStorage.getItem("unrelated")).toBe("keep");
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("删除写入失败与清空访问失败不会伪报成功", () => {
    localStorage.setItem(
      SHORT_LINK_HISTORY_KEY,
      JSON.stringify([record(1, "2026-01-01T00:00:00.000Z")]),
    );
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("quota", "QuotaExceededError");
    });

    expect(deleteShortLinkHistory("id-1")).toEqual({
      status: "failure",
      records: [record(1, "2026-01-01T00:00:00.000Z")],
      error: { code: "write_failed" },
    });
    vi.restoreAllMocks();

    vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new DOMException("denied", "SecurityError");
    });
    expect(clearShortLinkHistory()).toEqual({
      status: "failure",
      records: [],
      error: { code: "write_failed" },
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
