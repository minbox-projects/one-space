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
      "ai-routing-gateway": true,
    });
  });

  it("允许显式隐藏并重新显示短链接工具", () => {
    setLauncherToolVisible("short-link", false);
    expect(isLauncherToolVisible("short-link")).toBe(false);

    setLauncherToolVisible("short-link", true);
    expect(isLauncherToolVisible("short-link")).toBe(true);
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
      "ai-routing-gateway": true,
    });
  });
});
