import { describe, expect, it } from "vitest";
import i18n from "@/i18n";

const FILE_SHARING_KEYS = [
  "fileSharing",
  "fileSharingDesc",
  "fileSharingLauncherDesc",
  "fileSharingChooseFiles",
  "fileSharingNetwork",
  "fileSharingStart",
  "fileSharingStop",
  "fileSharingWarning",
  "fileSharingState_completed",
] as const;

describe("文件共享国际化", () => {
  it.each(["en", "zh"] as const)("为 %s 提供关键界面文案", async (language) => {
    await i18n.changeLanguage(language);
    for (const key of FILE_SHARING_KEYS) expect(i18n.t(key)).not.toBe(key);
  });
});

describe("OpenCode 模型列表国际化", () => {
  it.each([
    ["en", "Model list", "models", "Toggle model options", "Toggle model variants"],
    ["zh", "模型列表", "个模型", "展开或收起模型选项", "展开或收起模型变体"],
  ] as const)("为 %s 提供专用文案且不改通用 models", async (language, modelList, models, toggleOptions, toggleVariants) => {
    await i18n.changeLanguage(language);
    expect(i18n.t("openCodeModelList")).toBe(modelList);
    expect(i18n.t("models")).toBe(models);
    expect(i18n.t("toggleModelOptions")).toBe(toggleOptions);
    expect(i18n.t("toggleModelVariants")).toBe(toggleVariants);
  });
});

function collectKeyPaths(value: unknown, prefix = "", paths: string[] = []): string[] {
  if (value && typeof value === "object") {
    for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
      const path = prefix ? `${prefix}.${key}` : key;
      paths.push(path);
      collectKeyPaths(child, path, paths);
    }
  }
  return paths;
}

function resourceBundle(language: "en" | "zh"): Record<string, unknown> {
  return i18n.getResourceBundle(language, "translation") as Record<string, unknown>;
}

describe("Antigravity 终端工具国际化键名", () => {
  it.each(["en", "zh"] as const)("为 %s 的键路径不含 gemini 片段", (language) => {
    const keyPaths = collectKeyPaths(resourceBundle(language));
    expect(keyPaths.length).toBeGreaterThan(0);
    const offending = keyPaths.filter((path) =>
      path
        .toLowerCase()
        .split(".")
        .some((segment) => segment.includes("gemini")),
    );
    expect(offending).toEqual([]);
  });

  it.each(["en", "zh"] as const)("为 %s 保留 AI 新闻关键词描述中的 Gemini 品牌引用", (language) => {
    expect(resourceBundle(language).newsKeywordsDesc).toContain("Gemini");
  });
});
