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

  it("en 与 zh 的键路径集合完全一致", () => {
    const enPaths = collectKeyPaths(resourceBundle("en"));
    const zhPaths = collectKeyPaths(resourceBundle("zh"));
    const enSet = new Set(enPaths);
    const zhSet = new Set(zhPaths);
    const onlyEn = enPaths.filter((path) => !zhSet.has(path));
    const onlyZh = zhPaths.filter((path) => !enSet.has(path));
    expect(
      { onlyEn, onlyZh },
      `en/zh 键路径差异: onlyEn=${JSON.stringify(onlyEn)} onlyZh=${JSON.stringify(onlyZh)}`,
    ).toEqual({ onlyEn: [], onlyZh: [] });
  });
});

const PROVIDER_TEMPLATE_KEYS = [
  "apiGatewayProviderTemplates",
  "apiGatewayProviderTemplatesDesc",
  "apiGatewayTemplateModelsCount",
  "apiGatewayTemplateSource",
  "apiGatewayTemplateLastSync",
  "apiGatewayTemplateNotSynced",
  "apiGatewayTemplateSync",
  "apiGatewayTemplateSyncing",
  "apiGatewayTemplateModelDisabled",
  "apiGatewayTemplateAddProvider",
  "apiGatewayTemplateExpand",
  "apiGatewayTemplateCollapse",
  "apiGatewayNoTemplates",
  "apiGatewayTemplatesLoadFailed",
  "apiGatewayTemplateNoModels",
  "apiGatewayTemplateSyncSuccess",
  "apiGatewayTemplateProviderCreated",
  "apiGatewayTemplateCreateFailed",
  "apiGatewayTemplateCreateTitle",
  "apiGatewayTemplateCreateDesc",
  "apiGatewayTemplateNameLabel",
  "apiGatewayTemplateIconLabel",
  "apiGatewayTemplateIconAuto",
  "apiGatewayTemplateIconOpenCode",
  "apiGatewayTemplateIconCommandCode",
  "apiGatewayTemplateIconOpenAI",
  "apiGatewayTemplateBaseUrlLabel",
  "apiGatewayTemplateProtocolLabel",
  "apiGatewayTemplateApiKeyLabel",
  "apiGatewayTemplateApiKeyRequired",
  "apiGatewayTemplateCreateSubmit",
  "apiGatewayEveryDay",
  "apiGatewayWeekdaySun",
  "apiGatewayWeekdayMon",
  "apiGatewayWeekdayTue",
  "apiGatewayWeekdayWed",
  "apiGatewayWeekdayThu",
  "apiGatewayWeekdayFri",
  "apiGatewayWeekdaySat",
  "apiGatewayTemplateDeprecated",
  "apiGatewayProviderTemplateAvatarTitle",
  "apiGatewayTemplateRetiredMappings",
  "apiGatewayTemplateRetiredMappingsTooltip",
  "apiGatewayIgnoredModels",
  "apiGatewayIgnoredModelsDesc",
  "apiGatewayRestoreModel",
  "apiGatewayReasoningEfforts",
  "apiGatewayReasoningEffortsDesc",
  "apiGatewayReasoningEffortPlaceholder",
  "apiGatewayReasoningEffortAdd",
  "apiGatewayReasoningEffortRemove",
  "apiGatewayMappingDetails",
  "apiGatewayMappingDeleted",
  "apiGatewayModelRestored",
  "apiGatewayWeekdaySelect",
] as const;

describe("服务商模板国际化键", () => {
  it.each(["en", "zh"] as const)("为 %s 提供全部服务商模板键的真实文案", async (language) => {
    await i18n.changeLanguage(language);
    const fallbacks = PROVIDER_TEMPLATE_KEYS.filter((key) => i18n.t(key) === key);
    expect(fallbacks, `${language} 中回退为键名的服务商模板键`).toEqual([]);
  });

  it.each([
    ["en", "Provider Templates", "Upstream providers"],
    ["zh", "服务商模板", "上游服务商"],
  ] as const)("为 %s 保留模板与上游服务商命名", async (language, templates, providers) => {
    await i18n.changeLanguage(language);
    expect(i18n.t("apiGatewayProviderTemplates")).toBe(templates);
    expect(i18n.t("apiGatewayProviders")).toBe(providers);
  });

  it("zh 提供每天与同步中文案", async () => {
    await i18n.changeLanguage("zh");
    expect(i18n.t("apiGatewayEveryDay")).toBe("每天");
    expect(i18n.t("apiGatewayTemplateSyncing")).toBe("同步中...");
  });

  it.each([
    ["en", "2 mapping(s) removed from template"],
    ["zh", "2 个映射已从模板移除"],
  ] as const)("为 %s 的退休映射提示保留 {{count}} 并正常插值", async (language, expected) => {
    await i18n.changeLanguage(language);
    const raw = resourceBundle(language).apiGatewayTemplateRetiredMappings;
    expect(typeof raw).toBe("string");
    expect(raw as string).toContain("{{count}}");
    expect(i18n.t("apiGatewayTemplateRetiredMappings", { count: 2 })).toBe(
      expected,
    );
  });

  it.each([
    ["en", "Sync models", "Disabled"],
    ["zh", "同步模型列表", "已禁用"],
  ] as const)(
    "为 %s 提供同步模型列表与模型禁用状态真实文案",
    async (language, syncLabel, disabledLabel) => {
      await i18n.changeLanguage(language);
      expect(i18n.t("apiGatewayTemplateSync")).toBe(syncLabel);
      expect(i18n.t("apiGatewayTemplateModelDisabled")).toBe(disabledLabel);
      expect(i18n.t("apiGatewayTemplateSyncing")).not.toBe(
        "apiGatewayTemplateSyncing",
      );
    },
  );

  it.each(["en", "zh"] as const)("为 %s 的模型数量文案保留 {{count}} 并正常插值", async (language) => {
    await i18n.changeLanguage(language);
    const raw = resourceBundle(language).apiGatewayTemplateModelsCount;
    expect(typeof raw).toBe("string");
    expect(raw as string).toContain("{{count}}");
    expect(i18n.t("apiGatewayTemplateModelsCount", { count: 3 })).not.toContain("{{count}}");
  });
});

const PROVIDER_MAPPING_PRICE_KEYS = [
  "apiGatewayMappingPrice",
  "apiGatewayDefaultModelNone",
  "apiGatewayDefaultModelAutoAdded",
] as const;

describe("服务商映射价格国际化键", () => {
  it.each(["en", "zh"] as const)("为 %s 提供映射价格真实文案", async (language) => {
    await i18n.changeLanguage(language);
    const fallbacks = PROVIDER_MAPPING_PRICE_KEYS.filter(
      (key) => i18n.t(key) === key,
    );
    expect(fallbacks, `${language} 中回退为键名的映射价格键`).toEqual([]);
  });
});

const REMOVED_LEGACY_MODEL_PRICE_KEYS = [
  "apiGatewayAddPrice",
  "apiGatewayDeletePriceAria",
  "apiGatewayModelPriceDialogTitle",
  "apiGatewayModelPriceDialogDesc",
  "apiGatewayModelPrices",
  "apiGatewayModelPricesDesc",
  "apiGatewayModelPricesEmpty",
  "apiGatewayModelPricesLoadFailed",
  "apiGatewayModelPricesSaveFailed",
  "apiGatewayModelPricesSaved",
  "apiGatewayOffPeakActive",
  "apiGatewayOffPeakBadge",
  "apiGatewayOffPeakCount",
  "apiGatewayOffPeakEmpty",
  "apiGatewayPriceAllModelsConfigured",
  "apiGatewayPriceModel",
  "apiGatewayPriceModelCount",
  "apiGatewayPriceModelCount_plural",
  "apiGatewayPriceModelPlaceholder",
  "apiGatewayPriceNoAvailableModels",
  "apiGatewayPriceNoProviders",
  "apiGatewayPriceProviderNoModels",
  "apiGatewayPriceSelectModel",
  "apiGatewayPriceUnassignedProvider",
] as const;

const KEPT_PROVIDER_PRICE_KEYS = [
  "apiGatewayMappingPrice",
  "apiGatewayDefaultModelNone",
  "apiGatewayDefaultModelAutoAdded",
  "apiGatewayPriceInput",
  "apiGatewayPriceCacheRead",
  "apiGatewayPriceCacheWrite",
  "apiGatewayPriceOutput",
  "apiGatewayPricePerMillion",
  "apiGatewayOffPeakEnable",
  "apiGatewayOffPeakConfigure",
  "apiGatewayOffPeakTimeRange",
  "apiGatewayOffPeakStartTime",
  "apiGatewayOffPeakEndTime",
  "apiGatewayOffPeakRates",
  "apiGatewayOffPeakAdd",
  "apiGatewayOffPeakWindowIndex",
  "apiGatewayEveryDay",
  "apiGatewayWeekdaySelect",
] as const;

describe("API 网关模型价格旧入口国际化键清理", () => {
  it.each(["en", "zh"] as const)(
    "为 %s 移除旧入口键并保留服务商弹窗价格键",
    async (language) => {
      await i18n.changeLanguage(language);
      for (const key of REMOVED_LEGACY_MODEL_PRICE_KEYS) {
        expect(i18n.t(key), `${language} 中旧键 ${key} 应回退为键名`).toBe(key);
      }
      for (const key of KEPT_PROVIDER_PRICE_KEYS) {
        expect(i18n.t(key), `${language} 中保留键 ${key} 应仍有文案`).not.toBe(key);
      }
    },
  );
});

const REMOVED_PROVIDER_TEMPLATE_KEYS = [
  "apiGatewayTemplateSnapshot",
  "apiGatewayTemplateSnapshotVersion",
  "apiGatewayTemplatePriceInput",
  "apiGatewayTemplatePriceCacheRead",
  "apiGatewayTemplatePriceCacheWrite",
  "apiGatewayTemplatePriceOutput",
  "apiGatewayTemplatePriceUnit",
  "apiGatewayTemplateOffPeak",
  "apiGatewayTemplateNoOffPeak",
  "apiGatewayTemplateReasoningEfforts",
  "apiGatewayTemplateFetchModels",
  "apiGatewayTemplateFetchingModels",
  "apiGatewayTemplateFetchModelsApiKey",
  "apiGatewayTemplateFetchModelsSuccess",
  "apiGatewayTemplateImportSelected",
  "apiGatewayTemplateSelectAll",
  "apiGatewayTemplateDeselectAll",
  "targetUrl",
  "alreadyAdded",
] as const;

describe("服务商模板快照、价格与获取模型旧键清理", () => {
  it.each(["en", "zh"] as const)("为 %s 移除快照、价格与获取模型键", async (language) => {
    await i18n.changeLanguage(language);
    const fallbacks = REMOVED_PROVIDER_TEMPLATE_KEYS.filter(
      (key) => i18n.t(key) !== key,
    );
    expect(
      fallbacks,
      `${language} 中应回退为键名的旧模板键`,
    ).toEqual([]);
  });
});

