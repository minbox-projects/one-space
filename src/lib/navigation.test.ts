import { describe, expect, it } from "vitest";
import { isMoreToolsTab, resolveNavigationTarget } from "@/lib/navigation";

describe("snippets and notes navigation", () => {
  it.each([
    ["snippets", "snippets"],
    ["notes", "notes"],
  ])("keeps %s as a standalone tab instead of a More Tools section", (target, tab) => {
    expect(resolveNavigationTarget(target)).toEqual({ tab });
    expect(isMoreToolsTab(target)).toBe(false);
  });
});

describe("API Gateway navigation", () => {
  it("resolves api-gateway to a standalone tab instead of a More Tools section", () => {
    expect(resolveNavigationTarget("api-gateway")).toEqual({ tab: "api-gateway" });
    expect(isMoreToolsTab("api-gateway")).toBe(false);
  });
});

describe("MD5 navigation", () => {
  it("resolves the shared MD5 tool target to its More Tools detail", () => {
    expect(resolveNavigationTarget("md5-encryption")).toEqual({
      tab: "more-tools",
      moreToolsSection: "md5-encryption",
    });
    expect(isMoreToolsTab("md5-encryption")).toBe(true);
  });
});

describe("JT/T data parser navigation", () => {
  it("resolves the total jtt-data-parser target without an optional subtab", () => {
    expect(resolveNavigationTarget("jtt-data-parser")).toEqual({
      tab: "more-tools",
      moreToolsSection: "jtt-data-parser",
    });
    expect(isMoreToolsTab("jtt-data-parser")).toBe(true);
  });

  it.each([
    ["808", "jt808"],
    ["809", "jt809"],
    ["1078", "jt1078"],
    ["hex", "hex"],
  ])("resolves the %s alias to the jtt-data-parser section with the %s subtab", (alias, subtab) => {
    expect(resolveNavigationTarget(alias)).toEqual({
      tab: "more-tools",
      moreToolsSection: "jtt-data-parser",
      jttParserTab: subtab,
    });
  });

  it("keeps unrelated aliases resolving to their existing sections", () => {
    expect(resolveNavigationTarget("json-parser")).toEqual({
      tab: "more-tools",
      moreToolsSection: "json-parser",
    });
    expect(resolveNavigationTarget("bookmarks")).toEqual({
      tab: "more-tools",
      moreToolsSection: "bookmarks",
    });
  });
});

describe("AI Workflow model switcher navigation", () => {
  it("resolves the ai-workflow-model-switcher target to its More Tools detail", () => {
    expect(resolveNavigationTarget("ai-workflow-model-switcher")).toEqual({
      tab: "more-tools",
      moreToolsSection: "ai-workflow-model-switcher",
    });
    expect(isMoreToolsTab("ai-workflow-model-switcher")).toBe(true);
  });
});

