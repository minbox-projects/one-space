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
