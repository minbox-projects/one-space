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
    ["en", "Model list", "models"],
    ["zh", "模型列表", "个模型"],
  ] as const)("为 %s 提供专用文案且不改通用 models", async (language, modelList, models) => {
    await i18n.changeLanguage(language);
    expect(i18n.t("openCodeModelList")).toBe(modelList);
    expect(i18n.t("models")).toBe(models);
  });
});

const AI_GATEWAY_KEYS = [
  "aiRoutingGateway.title",
  "aiRoutingGateway.tabs.home",
  "aiRoutingGateway.tabs.accounts",
  "aiRoutingGateway.tabs.keys",
  "aiRoutingGateway.tabs.logs",
  "aiRoutingGateway.tabs.settings",
  "aiRoutingGateway.states.locked",
  "aiRoutingGateway.states.portConflict",
  "aiRoutingGateway.accounts.oauth.loopback",
  "aiRoutingGateway.keys.oneTimeTitle",
  "aiRoutingGateway.keys.manageGroups",
  "aiRoutingGateway.keys.statuses.active",
  "aiRoutingGateway.keys.convertTitle",
  "aiRoutingGateway.keys.tools.opencode",
  "aiRoutingGateway.logs.noAttempts",
  "aiRoutingGateway.settings.maintenance",
] as const;

describe("AI 路由网关国际化", () => {
  it.each(["en", "zh"] as const)("为 %s 提供完整关键界面文案", async (language) => {
    await i18n.changeLanguage(language);
    for (const key of AI_GATEWAY_KEYS) expect(i18n.t(key)).not.toBe(key);
  });

  it("中英文新增资源键集合一致", () => {
    const keys = (language: "en" | "zh") => {
      const value = i18n.getResourceBundle(language, "translation").aiRoutingGateway;
      const output: string[] = [];
      const visit = (node: unknown, prefix = "") => {
        if (!node || typeof node !== "object") return;
        for (const [key, child] of Object.entries(node)) {
          const path = prefix ? `${prefix}.${key}` : key;
          if (child && typeof child === "object") visit(child, path);
          else output.push(path);
        }
      };
      visit(value);
      return output.sort();
    };
    expect(keys("zh")).toEqual(keys("en"));
  });
});
