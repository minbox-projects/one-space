import { beforeEach, describe, expect, it } from "vitest";
import {
  LAUNCHER_TOOL_VISIBILITY_KEY,
  isLauncherToolVisible,
  readLauncherToolVisibility,
  setLauncherToolVisible,
} from "@/lib/launcherToolVisibility";

type RegistryToolDescriptor = {
  id: string;
  defaultVisible: boolean;
  surfaces: readonly string[];
};

type RegistryModule = {
  listToolboxTools: (surface: string) => RegistryToolDescriptor[];
  getToolboxTool: (id: string) => RegistryToolDescriptor | undefined;
};

/**
 * Loaded lazily so this suite still runs the task-001 guards while the
 * registry module is unavailable.
 */
async function loadRegistry(): Promise<RegistryModule> {
  const specifier = ["@/toolbox", "registry"].join("/");
  return (await import(/* @vite-ignore */ specifier)) as RegistryModule;
}

describe("launcherToolVisibility", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("新安装默认显示短链接工具", () => {
    expect(readLauncherToolVisibility()["short-link"]).toBe(true);
    expect(isLauncherToolVisible("short-link")).toBe(true);
  });

  it("以完整默认值补充旧对象中的新增字段并保留有效显式偏好", async () => {
    const registry = await loadRegistry();
    const md5Tool = registry.getToolboxTool("md5-encryption");
    expect(md5Tool).toBeDefined();
    const explicitQuickId = registry
      .listToolboxTools("launcher-quick")
      .map((tool) => tool.id)
      .find(
        (id) =>
          ![
            "bookmarks",
            "protocol-router",
            "json-parser",
            "short-link",
            md5Tool!.id,
          ].includes(id),
      );
    expect(explicitQuickId).toBeDefined();

    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({
        bookmarks: false,
        [explicitQuickId!]: true,
        "protocol-router": false,
        "json-parser": "false",
      }),
    );

    expect(readLauncherToolVisibility()).toMatchObject({
      bookmarks: false,
      [explicitQuickId!]: true,
      "protocol-router": false,
      "json-parser": true,
      [md5Tool!.id]: true,
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

  it("新安装默认显示协议路由工具", () => {
    expect(readLauncherToolVisibility()["protocol-router"]).toBe(true);
    expect(isLauncherToolVisible("protocol-router")).toBe(true);
  });

  it("允许显式隐藏并重新显示协议路由工具", () => {
    setLauncherToolVisible("protocol-router", false);
    expect(isLauncherToolVisible("protocol-router")).toBe(false);

    setLauncherToolVisible("protocol-router", true);
    expect(isLauncherToolVisible("protocol-router")).toBe(true);
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
  ])("对%s配置沿用完整默认值回退", async (_label, storedValue) => {
    const registry = await loadRegistry();
    const md5Tool = registry.getToolboxTool("md5-encryption");
    expect(md5Tool).toBeDefined();
    if (storedValue !== null) {
      localStorage.setItem(LAUNCHER_TOOL_VISIBILITY_KEY, storedValue);
    }

    expect(readLauncherToolVisibility()).toMatchObject({
      bookmarks: true,
      "protocol-router": true,
      [md5Tool!.id]: true,
      "short-link": true,
    });
  });

  it("新安装默认显示 AI Workflow 模型切换工具", () => {
    expect(
      readLauncherToolVisibility()["ai-workflow-model-switcher"],
    ).toBe(true);
    expect(
      isLauncherToolVisible("ai-workflow-model-switcher"),
    ).toBe(true);
  });

  it("允许显式隐藏并重新显示 AI Workflow 模型切换工具", () => {
    setLauncherToolVisible("ai-workflow-model-switcher", false);
    expect(
      isLauncherToolVisible("ai-workflow-model-switcher"),
    ).toBe(false);

    setLauncherToolVisible("ai-workflow-model-switcher", true);
    expect(
      isLauncherToolVisible("ai-workflow-model-switcher"),
    ).toBe(true);
  });

  it("以 kebab-case 记录隐藏 MD5 工具", () => {
    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({ "md5-encryption": false }),
    );

    const visibility = readLauncherToolVisibility() as unknown as Record<
      string,
      boolean | undefined
    >;
    expect(visibility["md5-encryption"]).toBe(false);
  });

  it("忽略遗留 camelCase MD5 记录并回退到默认显示", () => {
    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({ md5Encryption: false }),
    );

    const visibility = readLauncherToolVisibility() as unknown as Record<
      string,
      boolean | undefined
    >;
    expect(visibility["md5-encryption"]).toBe(true);
  });
});

describe("registry-derived visibility defaults", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("derives every launcher-quick tool default from the registry", async () => {
    const registry = await loadRegistry();
    const quickTools = registry.listToolboxTools("launcher-quick");
    expect(quickTools.length).toBeGreaterThan(0);

    const visibility = readLauncherToolVisibility() as unknown as Record<
      string,
      boolean | undefined
    >;
    for (const tool of quickTools) {
      expect(visibility[tool.id]).toBe(tool.defaultVisible);
      expect(
        isLauncherToolVisible(
          tool.id as unknown as Parameters<typeof isLauncherToolVisible>[0],
        ),
      ).toBe(tool.defaultVisible);
    }
  });

  it("falls back to registry defaults for launcher-quick tools on corruption", async () => {
    const registry = await loadRegistry();
    localStorage.setItem(LAUNCHER_TOOL_VISIBILITY_KEY, "{ not json");

    const visibility = readLauncherToolVisibility() as unknown as Record<
      string,
      boolean | undefined
    >;
    for (const tool of registry.listToolboxTools("launcher-quick")) {
      expect(visibility[tool.id]).toBe(tool.defaultVisible);
    }
  });
});

