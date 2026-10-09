import { describe, expect, it } from "vitest";
import { formatDateTime, formatTimeHms } from "./gatewayShared";

describe("gatewayShared formatDateTime", () => {
  it("空值或无效输入返回 null", () => {
    expect(formatDateTime(null)).toBeNull();
    expect(formatDateTime(undefined)).toBeNull();
    expect(formatDateTime(Number.NaN)).toBeNull();
    expect(formatDateTime(new Date("invalid-date"))).toBeNull();
  });

  it("正确格式化毫秒级时间戳为 YYYY-MM-DD HH:mm:ss", () => {
    // 构造特定时间对象
    const date = new Date(2026, 9, 9, 15, 8, 7); // 2026-10-09 15:08:07 (月份 0-indexed)
    const result = formatDateTime(date.getTime());
    expect(result).toBe("2026-10-09 15:08:07");
  });

  it("正确格式化 Date 对象为 YYYY-MM-DD HH:mm:ss", () => {
    const date = new Date(2025, 0, 5, 9, 4, 3); // 2025-01-05 09:04:03
    const result = formatDateTime(date);
    expect(result).toBe("2025-01-05 09:04:03");
  });

  it("保持 formatTimeHms 兼容性", () => {
    const date = new Date(2026, 9, 9, 15, 8, 7);
    const result = formatTimeHms(date.getTime());
    expect(result).toMatch(/\d{2}:\d{2}:\d{2}/);
  });
});
