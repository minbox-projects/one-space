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

// REQ-007 / AC-011 guard: the plan removes these unreferenced bilingual keys.
const REMOVED_AI_GATEWAY_BILINGUAL_KEYS = [
  "aiGatewayStatusCancelled",
  "aiGatewayProviderTemplatesDesc",
  "aiGatewayTemplateProtocolChat",
  "aiGatewayTemplateProtocolResponses",
  "aiGateway.provider.weightInvalid",
] as const;

describe("AI 网关遗留双语文案键移除", () => {
  it.each(["en", "zh"] as const)("为 %s 移除未引用的网关双语键", (language) => {
    const keyPaths = new Set(collectKeyPaths(resourceBundle(language)));
    for (const key of REMOVED_AI_GATEWAY_BILINGUAL_KEYS) {
      expect(
        keyPaths.has(key),
        `${language} 中已移除键 ${key} 不应存在`,
      ).toBe(false);
    }
  });

  it("移除后 en 与 zh 的键路径集合仍完全一致", () => {
    const enPaths = collectKeyPaths(resourceBundle("en"));
    const zhPaths = collectKeyPaths(resourceBundle("zh"));
    const enSet = new Set(enPaths);
    const zhSet = new Set(zhPaths);
    const onlyEn = enPaths.filter((path) => !zhSet.has(path));
    const onlyZh = zhPaths.filter((path) => !enSet.has(path));
    expect({ onlyEn, onlyZh }).toEqual({ onlyEn: [], onlyZh: [] });
  });
});

const PROVIDER_TEMPLATE_KEYS = [
  "aiGatewayProviderTemplates",
  "aiGatewayTemplateModelsCount",
  "aiGatewayTemplateSource",
  "aiGatewayTemplateLastSync",
  "aiGatewayTemplateNotSynced",
  "aiGatewayTemplateSync",
  "aiGatewayTemplateSyncing",
  "aiGatewayTemplateModelDisabled",
  "aiGatewayTemplateAddProvider",
  "aiGatewayTemplateExpand",
  "aiGatewayTemplateTab",
  "aiGatewayBoundTemplate",
  "aiGatewayBoundTemplateBadge",
  "aiGatewayBoundTemplateNotFound",
  "aiGatewayBoundTemplatePresetModels",
  "aiGatewayConfiguredModelCount",
  "aiGatewayNoTemplates",
  "aiGatewayTemplatesLoadFailed",
  "aiGatewayTemplateNoModels",
  "aiGatewayTemplateSyncSuccess",
  "aiGatewayTemplateProviderCreated",
  "aiGatewayTemplateCreateFailed",
  "aiGatewayTemplateCreateTitle",
  "aiGatewayTemplateCreateDesc",
  "aiGatewayTemplateNameLabel",
  "aiGatewayTemplateIconLabel",
  "aiGatewayTemplateIconAuto",
  "aiGatewayTemplateIconOpenCode",
  "aiGatewayTemplateIconCommandCode",
  "aiGatewayTemplateIconOpenAI",
  "aiGatewayTemplateBaseUrlLabel",
  "aiGatewayTemplateProtocolLabel",
  "aiGatewayTemplateApiKeyLabel",
  "aiGatewayTemplateApiKeyRequired",
  "aiGatewayTemplateCreateSubmit",
  "aiGatewayEveryDay",
  "aiGatewayWeekdaySun",
  "aiGatewayWeekdayMon",
  "aiGatewayWeekdayTue",
  "aiGatewayWeekdayWed",
  "aiGatewayWeekdayThu",
  "aiGatewayWeekdayFri",
  "aiGatewayWeekdaySat",
  "aiGatewayTemplateDeprecated",
  "aiGatewayProviderTemplateAvatarTitle",
  "aiGatewayTemplateRetiredMappings",
  "aiGatewayTemplateRetiredMappingsTooltip",
  "aiGatewayIgnoredModels",
  "aiGatewayIgnoredModelsDesc",
  "aiGatewayRestoreModel",
  "aiGatewayReasoningEfforts",
  "aiGatewayReasoningEffortsDesc",
  "aiGatewayReasoningEffortPlaceholder",
  "aiGatewayReasoningEffortAdd",
  "aiGatewayReasoningEffortRemove",
  "aiGatewayMappingDetails",
  "aiGatewayMappingDeleted",
  "aiGatewayModelRestored",
  "aiGatewayWeekdaySelect",
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
    expect(i18n.t("aiGatewayProviderTemplates")).toBe(templates);
    expect(i18n.t("aiGatewayProviders")).toBe(providers);
  });

  it("zh 提供每天与同步中文案", async () => {
    await i18n.changeLanguage("zh");
    expect(i18n.t("aiGatewayEveryDay")).toBe("每天");
    expect(i18n.t("aiGatewayTemplateSyncing")).toBe("同步中...");
  });

  it.each([
    ["en", "2 mapping(s) removed from template"],
    ["zh", "2 个映射已从模板移除"],
  ] as const)("为 %s 的退休映射提示保留 {{count}} 并正常插值", async (language, expected) => {
    await i18n.changeLanguage(language);
    const raw = resourceBundle(language).aiGatewayTemplateRetiredMappings;
    expect(typeof raw).toBe("string");
    expect(raw as string).toContain("{{count}}");
    expect(i18n.t("aiGatewayTemplateRetiredMappings", { count: 2 })).toBe(
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
      expect(i18n.t("aiGatewayTemplateSync")).toBe(syncLabel);
      expect(i18n.t("aiGatewayTemplateModelDisabled")).toBe(disabledLabel);
      expect(i18n.t("aiGatewayTemplateSyncing")).not.toBe(
        "aiGatewayTemplateSyncing",
      );
    },
  );

  it.each(["en", "zh"] as const)("为 %s 的模型数量文案保留 {{count}} 并正常插值", async (language) => {
    await i18n.changeLanguage(language);
    const raw = resourceBundle(language).aiGatewayTemplateModelsCount;
    expect(typeof raw).toBe("string");
    expect(raw as string).toContain("{{count}}");
    expect(i18n.t("aiGatewayTemplateModelsCount", { count: 3 })).not.toContain("{{count}}");
  });
});

const PROVIDER_MAPPING_PRICE_KEYS = [
  "aiGatewayMappingPrice",
  "aiGatewayDefaultModelNone",
  "aiGatewayDefaultModelAutoAdded",
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
  "aiGatewayAddPrice",
  "aiGatewayDeletePriceAria",
  "aiGatewayModelPriceDialogTitle",
  "aiGatewayModelPriceDialogDesc",
  "aiGatewayModelPrices",
  "aiGatewayModelPricesDesc",
  "aiGatewayModelPricesEmpty",
  "aiGatewayModelPricesLoadFailed",
  "aiGatewayModelPricesSaveFailed",
  "aiGatewayModelPricesSaved",
  "aiGatewayOffPeakActive",
  "aiGatewayOffPeakBadge",
  "aiGatewayOffPeakCount",
  "aiGatewayOffPeakEmpty",
  "aiGatewayPriceAllModelsConfigured",
  "aiGatewayPriceModel",
  "aiGatewayPriceModelCount",
  "aiGatewayPriceModelCount_plural",
  "aiGatewayPriceModelPlaceholder",
  "aiGatewayPriceNoAvailableModels",
  "aiGatewayPriceNoProviders",
  "aiGatewayPriceProviderNoModels",
  "aiGatewayPriceSelectModel",
  "aiGatewayPriceUnassignedProvider",
] as const;

const KEPT_PROVIDER_PRICE_KEYS = [
  "aiGatewayMappingPrice",
  "aiGatewayDefaultModelNone",
  "aiGatewayDefaultModelAutoAdded",
  "aiGatewayPriceInput",
  "aiGatewayPriceCacheRead",
  "aiGatewayPriceCacheWrite",
  "aiGatewayPriceOutput",
  "aiGatewayPricePerMillion",
  "aiGatewayOffPeakEnable",
  "aiGatewayOffPeakConfigure",
  "aiGatewayOffPeakTimeRange",
  "aiGatewayOffPeakStartTime",
  "aiGatewayOffPeakEndTime",
  "aiGatewayOffPeakRates",
  "aiGatewayOffPeakAdd",
  "aiGatewayOffPeakWindowIndex",
  "aiGatewayEveryDay",
  "aiGatewayWeekdaySelect",
] as const;

describe("AI 网关模型价格旧入口国际化键清理", () => {
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
  "aiGatewayTemplateSnapshot",
  "aiGatewayTemplateSnapshotVersion",
  "aiGatewayTemplatePriceInput",
  "aiGatewayTemplatePriceCacheRead",
  "aiGatewayTemplatePriceCacheWrite",
  "aiGatewayTemplatePriceOutput",
  "aiGatewayTemplatePriceUnit",
  "aiGatewayTemplateOffPeak",
  "aiGatewayTemplateNoOffPeak",
  "aiGatewayTemplateReasoningEfforts",
  "aiGatewayTemplateFetchModels",
  "aiGatewayTemplateFetchingModels",
  "aiGatewayTemplateFetchModelsApiKey",
  "aiGatewayTemplateFetchModelsSuccess",
  "aiGatewayTemplateImportSelected",
  "aiGatewayTemplateSelectAll",
  "aiGatewayTemplateDeselectAll",
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

// ---------------------------------------------------------------------------
// Step 3: 逐行自动禁用新增国际化键
// ---------------------------------------------------------------------------

const STEP_3_AUTO_DISABLE_KEYS = [
  "aiGatewayProviderAutoDisabledModelsHint",
  "aiGatewayReenableMapping",
  "aiGatewayReenableAllMappings",
] as const;

describe("逐行自动禁用新增国际化键", () => {
  it.each(["en", "zh"] as const)(
    "为 %s 提供全部 Step 3 新增键的真实文案",
    async (language) => {
      await i18n.changeLanguage(language);
      const fallbacks = STEP_3_AUTO_DISABLE_KEYS.filter(
        (key) => i18n.t(key) === key,
      );
      expect(fallbacks, `${language} 中回退为键名的 Step 3 键`).toEqual([]);
    },
  );

  it("提示键支持 count 插值显示被自动禁用的映射数量", async () => {
    await i18n.changeLanguage("en");
    const rawEn = resourceBundle("en").aiGatewayProviderAutoDisabledModelsHint;
    expect(typeof rawEn).toBe("string");
    expect(rawEn as string).toContain("{{count}}");
    expect(i18n.t("aiGatewayProviderAutoDisabledModelsHint", { count: 0 })).not.toContain("{{count}}");
    expect(i18n.t("aiGatewayProviderAutoDisabledModelsHint", { count: 5 })).not.toContain("{{count}}");
  });

  it.each([
    ["en", "Re-enable mapping"],
    ["zh", "重新启用映射"],
  ] as const)("重新启用映射按钮文案为 %s", async (language, expected) => {
    await i18n.changeLanguage(language);
    expect(i18n.t("aiGatewayReenableMapping")).toBe(expected);
  });

  it.each([
    ["en", "Re-enable all mappings"],
    ["zh", "重新启用所有映射"],
  ] as const)("重新启用全部映射按钮文案为 %s", async (language, expected) => {
    await i18n.changeLanguage(language);
    expect(i18n.t("aiGatewayReenableAllMappings")).toBe(expected);
  });

  it("en 与 zh 的键路径集合仍保持一致", () => {
    const enPaths = collectKeyPaths(resourceBundle("en"));
    const zhPaths = collectKeyPaths(resourceBundle("zh"));
    const onlyEn = enPaths.filter((path) => !new Set(zhPaths).has(path));
    const onlyZh = zhPaths.filter((path) => !new Set(enPaths).has(path));
    expect({ onlyEn, onlyZh }).toEqual({ onlyEn: [], onlyZh: [] });
  });
});

const USAGE_RANGE_YESTERDAY_KEYS = ["aiGatewayRangeYesterday"] as const;

describe("AI 网关快捷范围国际化键", () => {
  it.each(["en", "zh"] as const)(
    "为 %s 提供昨天范围的真实文案",
    async (language) => {
      await i18n.changeLanguage(language);
      for (const key of USAGE_RANGE_YESTERDAY_KEYS) {
        expect(i18n.t(key), `${language} 中 ${key} 应有真实文案`).not.toBe(key);
      }
    },
  );
});

const TEMPLATE_SYNC_NOTIFICATION_KEYS = [
  "aiGatewayTemplateSyncNotificationTitle",
  "aiGatewayTemplateSyncNotificationProviderCount",
  "aiGatewayTemplateSyncNotificationAddedCount",
  "aiGatewayTemplateSyncNotificationDisabledCount",
  "aiGatewayTemplateSyncNotificationDetailProvider",
  "messageSource_ai_gateway",
] as const;

const TEMPLATE_SYNC_COUNT_KEYS = [
  "aiGatewayTemplateSyncNotificationProviderCount",
  "aiGatewayTemplateSyncNotificationAddedCount",
  "aiGatewayTemplateSyncNotificationDisabledCount",
] as const;

describe("服务商模板自动同步通知国际化键", () => {
  it.each(["en", "zh"] as const)(
    "为 %s 提供全部自动同步通知键的真实文案",
    async (language) => {
      await i18n.changeLanguage(language);
      const fallbacks = TEMPLATE_SYNC_NOTIFICATION_KEYS.filter(
        (key) => i18n.t(key) === key,
      );
      expect(fallbacks, `${language} 中回退为键名的通知键`).toEqual([]);
    },
  );

  it.each(["en", "zh"] as const)(
    "为 %s 的通知计数键保留 {{count}} 并正常插值",
    async (language) => {
      await i18n.changeLanguage(language);
      for (const key of TEMPLATE_SYNC_COUNT_KEYS) {
        const raw = resourceBundle(language)[key];
        expect(typeof raw, `${language} 中 ${key} 应为字符串`).toBe("string");
        expect(
          raw as string,
          `${language} 中 ${key} 应保留 {{count}}`,
        ).toContain("{{count}}");
        expect(
          i18n.t(key, { count: 2 }),
          `${language} 中 ${key} 插值后不应残留 {{count}}`,
        ).not.toContain("{{count}}");
      }
    },
  );

  it.each(["en", "zh"] as const)(
    "为 %s 的通知标题保留 {{template}} 并正常插值",
    async (language) => {
      await i18n.changeLanguage(language);
      const raw = resourceBundle(language).aiGatewayTemplateSyncNotificationTitle;
      expect(typeof raw, `${language} 中通知标题应为字符串`).toBe("string");
      expect(raw as string).toContain("{{template}}");
      expect(
        i18n.t("aiGatewayTemplateSyncNotificationTitle", {
          template: "opencode-zen",
        }),
      ).toContain("opencode-zen");
    },
  );

  it.each(["en", "zh"] as const)(
    "为 %s 的通知明细键插值 provider 与 models",
    async (language) => {
      await i18n.changeLanguage(language);
      const detail = i18n.t("aiGatewayTemplateSyncNotificationDetailProvider", {
        provider: "Zen Upstream",
        models: "m, n",
      });
      expect(detail).toContain("Zen Upstream");
      expect(detail).toContain("m, n");
    },
  );
});

