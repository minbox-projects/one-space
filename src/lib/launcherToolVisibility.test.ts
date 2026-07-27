import { beforeEach, describe, expect, it } from "vitest";
import {
  LAUNCHER_TOOL_VISIBILITY_KEY,
  readLauncherToolVisibility,
} from "@/lib/launcherToolVisibility";

describe("launcherToolVisibility", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("为旧版可见性对象补充默认显示的 AI 请求抓包开关，同时保留已有选择", () => {
    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({ bookmarks: false, "protocol-router": false }),
    );

    expect(readLauncherToolVisibility()).toMatchObject({
      bookmarks: false,
      "protocol-router": false,
      "ai-request-capture": true,
    });
  });
});
