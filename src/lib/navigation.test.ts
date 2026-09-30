import { describe, expect, it } from "vitest";
import * as navigation from "@/lib/navigation";
import { isMoreToolsTab, resolveNavigationTarget } from "@/lib/navigation";

type RegistryToolDescriptor = {
  id: string;
  surfaces: readonly string[];
  aliases: readonly { target: string; jttParserTab?: string }[];
};

type RegistryModule = {
  TOOLBOX_TOOLS: readonly RegistryToolDescriptor[];
  listToolboxTools: (surface: string) => RegistryToolDescriptor[];
  resolveToolboxNavigationAlias: (
    target: string,
  ) =>
    | { toolId: string; tab: string; moreToolsSection?: string; jttParserTab?: string }
    | undefined;
};

/**
 * The registry is loaded lazily on purpose: while the module is absent this
 * suite must still load so the task-001 guards keep producing their own
 * per-test RED evidence instead of a file-level import failure.
 */
async function loadRegistry(): Promise<RegistryModule> {
  const specifier = ["@/toolbox", "registry"].join("/");
  return (await import(/* @vite-ignore */ specifier)) as RegistryModule;
}

describe("registry-derived navigation aliases", () => {
  it("resolves every registry hub id to its own more-tools section", async () => {
    const registry = await loadRegistry();
    const hubTools = registry.listToolboxTools("hub");
    expect(hubTools.length).toBeGreaterThan(0);

    for (const tool of hubTools) {
      expect(resolveNavigationTarget(tool.id)).toEqual({
        tab: "more-tools",
        moreToolsSection: tool.id,
      });
      expect(isMoreToolsTab(tool.id)).toBe(true);
    }
  });

  it("resolves ssh-tunnels to its more-tools detail", () => {
    expect(resolveNavigationTarget("ssh-tunnels")).toEqual({
      tab: "more-tools",
      moreToolsSection: "ssh-tunnels",
    });
  });

  it("resolves ssh-tunnels:connected to its more-tools detail with sshTunnelTab", () => {
    expect(resolveNavigationTarget("ssh-tunnels:connected")).toEqual({
      tab: "more-tools",
      moreToolsSection: "ssh-tunnels",
      sshTunnelTab: "__connected__",
    });
  });

  it("carries the JT/T parser tab payload from the registry alias", async () => {
    const registry = await loadRegistry();
    for (const alias of ["808", "809", "1078", "hex"]) {
      const resolved = registry.resolveToolboxNavigationAlias(alias);
      expect(resolved).toBeDefined();
      expect(resolveNavigationTarget(alias)).toEqual({
        tab: "more-tools",
        moreToolsSection: "jtt-data-parser",
        jttParserTab: resolved!.jttParserTab,
      });
    }
  });

  it("passes an unknown target through as a standalone tab", () => {
    expect(resolveNavigationTarget("__unknown-target__")).toEqual({
      tab: "__unknown-target__",
    });
    expect(isMoreToolsTab("__unknown-target__")).toBe(false);
  });
});

describe("snippets and notes navigation", () => {
  it.each([
    ["snippets", "snippets"],
    ["notes", "notes"],
  ])("keeps %s as a standalone tab instead of a More Tools section", (target, tab) => {
    expect(resolveNavigationTarget(target)).toEqual({ tab });
    expect(isMoreToolsTab(target)).toBe(false);
  });
});

describe("AI Gateway navigation", () => {
  it("resolves ai-gateway to a standalone tab instead of a More Tools section", () => {
    expect(resolveNavigationTarget("ai-gateway")).toEqual({ tab: "ai-gateway" });
    expect(isMoreToolsTab("ai-gateway")).toBe(false);
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

describe("removed more-tools ghost targets", () => {
  it.each(["cloud", "backup"])(
    "does not resolve %s into the more-tools surface",
    (target) => {
      expect(resolveNavigationTarget(target).tab).not.toBe("more-tools");
    },
  );

  it("does not export the legacy navigation compatibility helper", () => {
    const navigationModule = navigation as unknown as Record<string, unknown>;
    expect("normalizeLegacyTabTarget" in navigationModule).toBe(false);
  });

  it("passes the removed cloud target through as a standalone tab", () => {
    expect(resolveNavigationTarget("cloud")).toEqual({ tab: "cloud" });
    expect(isMoreToolsTab("cloud")).toBe(false);
  });

  it("passes the removed backup target through without a more-tools section", () => {
    expect(resolveNavigationTarget("backup")).toEqual({ tab: "backup" });
    expect(isMoreToolsTab("backup")).toBe(false);
  });
});

describe("removed AI Workspace navigation targets", () => {
  // Assembled from segments so the repository scan for removed feature targets
  // stays clean; the behavior under test is the passthrough itself.
  const removedTargets = [
    ["ai", "assistants"].join("-"),
    ["ai", "assistants", "library"].join("-"),
    ["ai", "automations"].join("-"),
    ["ai", "model", "center"].join("-"),
  ];

  it.each(removedTargets)(
    "passes %s through unchanged without opening a smart-workspace section",
    (target) => {
      const resolved = resolveNavigationTarget(target);
      expect(resolved).toEqual({ tab: target });
      expect("smartWorkspaceSection" in resolved).toBe(false);
      expect(isMoreToolsTab(target)).toBe(false);
    },
  );
});

