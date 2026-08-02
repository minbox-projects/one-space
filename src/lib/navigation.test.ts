import { describe, expect, it } from "vitest";
import { isMoreToolsTab, resolveNavigationTarget } from "@/lib/navigation";

describe("MD5 navigation", () => {
  it("resolves the shared MD5 tool target to its More Tools detail", () => {
    expect(resolveNavigationTarget("md5-encryption")).toEqual({
      tab: "more-tools",
      moreToolsSection: "md5-encryption",
    });
    expect(isMoreToolsTab("md5-encryption")).toBe(true);
  });
});

describe("AI routing gateway navigation", () => {
  it("resolves the isolated stable target to More Tools", () => {
    expect(resolveNavigationTarget("ai-routing-gateway")).toEqual({
      tab: "more-tools",
      moreToolsSection: "ai-routing-gateway",
    });
    expect(isMoreToolsTab("ai-routing-gateway")).toBe(true);
  });
});
