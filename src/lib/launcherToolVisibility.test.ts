import { beforeEach, describe, expect, it } from "vitest";
import {
  LAUNCHER_TOOL_VISIBILITY_KEY,
  isLauncherToolVisible,
  readLauncherToolVisibility,
  setLauncherToolVisible,
} from "@/lib/launcherToolVisibility";

describe("launcherToolVisibility", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("新安装默认显示短链接工具", () => {
    expect(readLauncherToolVisibility()["short-link"]).toBe(true);
    expect(isLauncherToolVisible("short-link")).toBe(true);
  });

  it("以完整默认值补充旧对象中的新增字段并保留有效显式偏好", () => {
    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({
        bookmarks: false,
        cloud: true,
        "protocol-router": false,
        "json-parser": "false",
      }),
    );

    expect(readLauncherToolVisibility()).toMatchObject({
      bookmarks: false,
      cloud: true,
      "protocol-router": false,
      "json-parser": true,
      md5Encryption: true,
      "short-link": true,
    });
  });

  it("允许显式隐藏并重新显示短链接工具", () => {
    setLauncherToolVisible("short-link", false);
    expect(isLauncherToolVisible("short-link")).toBe(false);

    setLauncherToolVisible("short-link", true);
    expect(isLauncherToolVisible("short-link")).toBe(true);
  });

  it("新安装默认显示 JT/T 数据解析工具", () => {
    expect(readLauncherToolVisibility()["jtt-data-parser"]).toBe(true);
    expect(isLauncherToolVisible("jtt-data-parser")).toBe(true);
  });

  it("以默认值补充缺失 JT/T 键的旧可见性记录", () => {
    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({ bookmarks: false }),
    );

    const visibility = readLauncherToolVisibility();
    expect(visibility.bookmarks).toBe(false);
    expect(visibility["jtt-data-parser"]).toBe(true);
  });

  it("忽略包含已废弃 JT/T 字段的可见性记录并回退到默认显示", () => {
    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({ jttParser: false, bookmarks: false }),
    );

    const visibility = readLauncherToolVisibility();
    expect(visibility["jtt-data-parser"]).toBe(true);
    expect(visibility.bookmarks).toBe(false);
  });

  it("允许显式隐藏并重新显示 JT/T 数据解析工具", () => {
    setLauncherToolVisible("jtt-data-parser", false);
    expect(isLauncherToolVisible("jtt-data-parser")).toBe(false);

    setLauncherToolVisible("jtt-data-parser", true);
    expect(isLauncherToolVisible("jtt-data-parser")).toBe(true);
  });

  it.each([
    [null, null],
    ["损坏 JSON", "{"],
  ])("对%s配置沿用完整默认值回退", (_label, storedValue) => {
    if (storedValue !== null) {
      localStorage.setItem(LAUNCHER_TOOL_VISIBILITY_KEY, storedValue);
    }

    expect(readLauncherToolVisibility()).toMatchObject({
      bookmarks: true,
      "protocol-router": true,
      md5Encryption: true,
      "short-link": true,
    });
  });

});
