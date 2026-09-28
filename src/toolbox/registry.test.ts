import { describe, expect, it } from "vitest";
import {
  TOOLBOX_TOOLS,
  assertToolboxRegistryIntegrity,
  getToolboxTool,
  listToolboxTools,
  resolveToolboxNavigationAlias,
} from "@/toolbox/registry";
import type {
  JttParserTabId,
  ToolboxNavAlias,
  ToolboxToolDescriptor,
} from "@/toolbox/types";

/**
 * Canonical facts from the frozen plan/task contract. These are the only
 * independent expected values; every other assertion derives from the
 * registry itself so a duplicated list is never maintained here.
 */
const CANONICAL_HUB_IDS = [
  "bookmarks",
  "cloud",
  "ssh",
  "ssh-tunnels",
  "protocol-router",
  "random-password",
  "json-parser",
  "md5-encryption",
  "short-link",
  "file-sharing",
  "jtt-data-parser",
  "ai-workflow-model-switcher",
] as const;

const NON_HUB_IDS = ["notes", "snippets"] as const;

const JTT_ALIAS_TABS: Array<[string, JttParserTabId]> = [
  ["808", "jt808"],
  ["809", "jt809"],
  ["1078", "jt1078"],
  ["hex", "hex"],
];

function requireHubTool(): ToolboxToolDescriptor {
  const tool = TOOLBOX_TOOLS.find((candidate) => candidate.surfaces.includes("hub"));
  if (!tool) throw new Error("registry exposes no hub tool");
  return tool;
}

describe("toolbox registry descriptors", () => {
  it("has at least one descriptor and unique ids", () => {
    expect(TOOLBOX_TOOLS.length).toBeGreaterThan(0);
    const ids = TOOLBOX_TOOLS.map((tool) => tool.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("contains exactly the canonical hub id set", () => {
    const hubIds = listToolboxTools("hub").map((tool) => tool.id);
    expect([...hubIds].sort()).toEqual([...CANONICAL_HUB_IDS].sort());
  });

  it("resolves every registered descriptor through getToolboxTool", () => {
    for (const tool of TOOLBOX_TOOLS) {
      expect(getToolboxTool(tool.id)).toBe(tool);
    }
    expect(getToolboxTool("__not-a-toolbox-tool__")).toBeUndefined();
  });

  it("exposes non-empty label and description keys and a component for every descriptor", () => {
    for (const tool of TOOLBOX_TOOLS) {
      expect(tool.labelKey.length).toBeGreaterThan(0);
      expect(tool.descriptionKey.length).toBeGreaterThan(0);
      expect(tool.component).toBeTruthy();
      expect(tool.icon).toBeTruthy();
      expect(typeof tool.iconClassName).toBe("string");
    }
  });

  it("uses only known surfaces", () => {
    const known = new Set([
      "hub",
      "launcher-quick",
      "launcher-internal",
      "sidebar",
      "tray",
    ]);
    for (const tool of TOOLBOX_TOOLS) {
      expect(tool.surfaces.length).toBeGreaterThan(0);
      for (const surface of tool.surfaces) {
        expect(known.has(surface)).toBe(true);
      }
    }
  });

  it("gives every descriptor a stable, unique default order and defaults to visible", () => {
    const orders = TOOLBOX_TOOLS.map((tool) => tool.defaultOrder);
    for (const order of orders) {
      expect(Number.isInteger(order)).toBe(true);
    }
    expect(new Set(orders).size).toBe(orders.length);
    for (const tool of TOOLBOX_TOOLS) {
      expect(tool.defaultVisible).toBe(true);
    }
  });
});

describe("listToolboxTools", () => {
  it("returns the registry order for a surface", () => {
    for (const surface of [
      "hub",
      "launcher-quick",
      "launcher-internal",
      "sidebar",
      "tray",
    ] as const) {
      expect(listToolboxTools(surface)).toEqual(
        TOOLBOX_TOOLS.filter((tool) => tool.surfaces.includes(surface)),
      );
    }
  });
});

describe("notes and snippets plugin surfaces", () => {
  it.each(NON_HUB_IDS)("registers %s on sidebar, tray and launcher-internal without a hub card", (id) => {
    const tool = getToolboxTool(id);
    expect(tool).toBeDefined();
    expect(tool!.surfaces).toEqual(
      expect.arrayContaining(["sidebar", "tray", "launcher-internal"]),
    );
    expect(tool!.surfaces).not.toContain("hub");
  });

  it("never lists notes or snippets in the hub surface", () => {
    const hubIds = listToolboxTools("hub").map((tool) => tool.id);
    for (const id of NON_HUB_IDS) {
      expect(hubIds).not.toContain(id);
    }
  });

  it("lists notes and snippets on the sidebar, tray and launcher-internal surfaces", () => {
    for (const surface of ["sidebar", "tray", "launcher-internal"] as const) {
      const ids = listToolboxTools(surface).map((tool) => tool.id);
      for (const id of NON_HUB_IDS) {
        expect(ids).toContain(id);
      }
    }
  });
});

describe("canonical MD5 id", () => {
  it("registers md5-encryption and no camelCase md5 descriptor", () => {
    expect(getToolboxTool("md5-encryption")).toBeDefined();
    expect(getToolboxTool("md5Encryption")).toBeUndefined();
    expect(TOOLBOX_TOOLS.some((tool) => tool.id === "md5Encryption")).toBe(false);
    expect(TOOLBOX_TOOLS.some((tool) => tool.id.toLowerCase() === "md5encryption")).toBe(
      false,
    );
  });

  it("keeps every hub id stable kebab-case", () => {
    for (const id of CANONICAL_HUB_IDS) {
      expect(id).toMatch(/^[a-z0-9]+(?:-[a-z0-9]+)*$/);
    }
  });
});

describe("navigation alias resolution", () => {
  it("resolves every hub id to its own more-tools target", () => {
    for (const tool of listToolboxTools("hub")) {
      const resolved = resolveToolboxNavigationAlias(tool.id);
      expect(resolved).toBeDefined();
      expect(resolved!.toolId).toBe(tool.id);
      expect(resolved!.tab).toBe("more-tools");
      expect(resolved!.moreToolsSection).toBe(tool.id);
      expect(resolved!.jttParserTab).toBeUndefined();
    }
  });

  it.each(JTT_ALIAS_TABS)(
    "resolves the %s alias to jtt-data-parser with the %s payload",
    (alias, tab) => {
      expect(resolveToolboxNavigationAlias(alias)).toEqual({
        toolId: "jtt-data-parser",
        tab: "more-tools",
        moreToolsSection: "jtt-data-parser",
        jttParserTab: tab,
      });
    },
  );

  it("gives the JTT tool an id alias without a tab payload", () => {
    expect(resolveToolboxNavigationAlias("jtt-data-parser")).toEqual({
      toolId: "jtt-data-parser",
      tab: "more-tools",
      moreToolsSection: "jtt-data-parser",
    });
  });

  it("returns undefined for an unknown target", () => {
    expect(resolveToolboxNavigationAlias("__unknown-target__")).toBeUndefined();
  });
});

describe("assertToolboxRegistryIntegrity", () => {
  it("accepts the real registry and an explicit empty list", () => {
    expect(() => assertToolboxRegistryIntegrity()).not.toThrow();
    expect(() => assertToolboxRegistryIntegrity(TOOLBOX_TOOLS)).not.toThrow();
    expect(() => assertToolboxRegistryIntegrity([])).not.toThrow();
  });

  it("throws on a duplicate descriptor id", () => {
    const base = requireHubTool();
    expect(() =>
      assertToolboxRegistryIntegrity([base, { ...base }]),
    ).toThrow();
  });

  it("throws on an unknown surface", () => {
    const base = requireHubTool();
    expect(() =>
      assertToolboxRegistryIntegrity([
        {
          ...base,
          id: "unknown-surface-tool",
          surfaces: [...base.surfaces, "bogus-surface" as never],
        },
      ]),
    ).toThrow();
  });

  it("throws on a hub tool id that is not stable kebab-case", () => {
    const base = requireHubTool();
    const invalid = {
      ...base,
      id: "md5Encryption",
      surfaces: ["hub"] as const,
    };
    expect(() => assertToolboxRegistryIntegrity([invalid])).toThrow();
  });

  it("throws on an alias without a target", () => {
    const base = requireHubTool();
    const aliasWithoutTarget = {} as ToolboxNavAlias;
    expect(() =>
      assertToolboxRegistryIntegrity([
        { ...base, id: "alias-less-tool", aliases: [aliasWithoutTarget] },
      ]),
    ).toThrow();
  });
});
