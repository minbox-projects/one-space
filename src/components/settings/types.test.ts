import { describe, expect, it } from "vitest";
import {
  MAIN_SETTINGS_CATEGORIES,
  resolveSettingsTab,
  type MainSettingsTab,
} from "./types";

function getCategory(id: string) {
  const category = MAIN_SETTINGS_CATEGORIES.find((entry) => entry.id === id);
  if (!category) {
    throw new Error(`Settings category "${id}" is not registered`);
  }
  return category;
}

describe("MAIN_SETTINGS_CATEGORIES", () => {
  it("lists the six main categories in display order", () => {
    expect(MAIN_SETTINGS_CATEGORIES.map((category) => category.id)).toEqual([
      "general",
      "ai",
      "services",
      "operations",
      "data-security",
      "news",
    ]);
  });

  it("limits the services category to the three network sub-tabs", () => {
    expect(getCategory("services").subTabs.map((subTab) => subTab.id)).toEqual([
      "proxy",
      "protocol-router",
      "ai-gateway",
    ]);
  });

  it("registers the four operations sub-tabs under the expected fallback title", () => {
    const operations = getCategory("operations");
    expect(operations.subTabs.map((subTab) => subTab.id)).toEqual([
      "ssl-certificate",
      "server-monitor",
      "cloud-agent",
      "remote-bot",
    ]);
    expect(operations.fallbackTitle).toBe("运维与自动化");
  });

  it("uses the dedicated category title keys in display order", () => {
    expect(MAIN_SETTINGS_CATEGORIES.map((category) => category.titleKey)).toEqual([
      "categoryGeneral",
      "categoryAi",
      "categoryNetwork",
      "categoryOperations",
      "categoryDataSecurity",
      "categoryNews",
    ]);
  });
});

describe("resolveSettingsTab", () => {
  it.each<[string | undefined, { mainTab: MainSettingsTab; subTab: string }]>([
    ["appearance", { mainTab: "general", subTab: "appearance" }],
    ["shortcuts", { mainTab: "general", subTab: "shortcuts" }],
    ["updates", { mainTab: "general", subTab: "updates" }],
    ["general", { mainTab: "general", subTab: "general" }],
    ["skills", { mainTab: "ai", subTab: "skills" }],
    ["subagents", { mainTab: "ai", subTab: "subagents" }],
    ["ai", { mainTab: "ai", subTab: "ai" }],
    ["proxy", { mainTab: "services", subTab: "proxy" }],
    ["protocol-router", { mainTab: "services", subTab: "protocol-router" }],
    ["ai-gateway", { mainTab: "services", subTab: "ai-gateway" }],
    ["ssl-certificate", { mainTab: "operations", subTab: "ssl-certificate" }],
    ["server-monitor", { mainTab: "operations", subTab: "server-monitor" }],
    ["cloud-agent", { mainTab: "operations", subTab: "cloud-agent" }],
    ["remote-bot", { mainTab: "operations", subTab: "remote-bot" }],
    ["services", { mainTab: "services", subTab: "proxy" }],
    ["storage", { mainTab: "data-security", subTab: "storage" }],
    ["security", { mainTab: "data-security", subTab: "security" }],
    ["data-security", { mainTab: "data-security", subTab: "storage" }],
    ["news", { mainTab: "news", subTab: "news" }],
    ["does-not-exist", { mainTab: "general", subTab: "general" }],
    [undefined, { mainTab: "general", subTab: "general" }],
  ])("routes %s to its category and sub-tab", (input, expected) => {
    expect(resolveSettingsTab(input)).toEqual(expected);
  });
});
