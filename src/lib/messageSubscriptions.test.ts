import { beforeEach, describe, expect, it } from "vitest";
import {
  FOCUSED_MESSAGE_TYPE_CATALOG,
  isMessageFocused,
  loadFocusedSubscriptionConfig,
  saveFocusedSubscriptionConfig,
} from "./messageSubscriptions";

describe("messageSubscriptions", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("provides comprehensive message catalog with recommended items", () => {
    expect(FOCUSED_MESSAGE_TYPE_CATALOG.length).toBeGreaterThan(10);
    const recommended = FOCUSED_MESSAGE_TYPE_CATALOG.filter((i) => i.isRecommended);
    expect(recommended.length).toBeGreaterThan(0);

    const ids = FOCUSED_MESSAGE_TYPE_CATALOG.map((i) => i.id);
    const uniqueIds = new Set(ids);
    expect(uniqueIds.size).toBe(ids.length); // 确保无重复 id
  });

  it("loads default recommended configuration when storage is empty", () => {
    const config = loadFocusedSubscriptionConfig();
    expect(config.subscribedTypeIds).toContain("ai_gateway:key_auth_failed");
    expect(config.subscribedTypeIds).toContain("uptime_monitor:status_down");
    expect(config.subscribedTypeIds).toContain("ssh_tunnels:disconnect");
    expect(config.preferences.highlightToast).toBe(true);
  });

  it("saves and loads custom configuration accurately", () => {
    const customConfig = {
      subscribedTypeIds: ["updater:available"],
      manualStarredIds: ["msg-999"],
      preferences: {
        highlightToast: false,
        extendedToastDuration: false,
      },
    };
    saveFocusedSubscriptionConfig(customConfig);

    const loaded = loadFocusedSubscriptionConfig();
    expect(loaded.subscribedTypeIds).toEqual(["updater:available"]);
    expect(loaded.manualStarredIds).toEqual(["msg-999"]);
    expect(loaded.preferences.highlightToast).toBe(false);
  });

  it("correctly identifies focused messages via subscriptions and manual stars", () => {
    const config = {
      subscribedTypeIds: ["ai_gateway:key_auth_failed"],
      manualStarredIds: ["custom-starred-1"],
      preferences: {
        highlightToast: true,
        extendedToastDuration: true,
      },
    };

    // 命中类型订阅
    expect(
      isMessageFocused(
        { id: "msg-1", source: "ai_gateway", category: "key_auth_failed" },
        config,
      ),
    ).toBe(true);

    // 未订阅类型
    expect(
      isMessageFocused(
        { id: "msg-2", source: "ai_gateway", category: "template_sync" },
        config,
      ),
    ).toBe(false);

    // 手动加星单条消息（即使其类别未被订阅）
    expect(
      isMessageFocused(
        { id: "custom-starred-1", source: "updater", category: "available" },
        config,
      ),
    ).toBe(true);
  });
});
