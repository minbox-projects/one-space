import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
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

// REQ-005 / AC-012 guard: the cleanup drops this unreferenced SSH tunnel key.
const REMOVED_SSH_TUNNEL_BILINGUAL_KEYS = [
  "sshTunnelGroupConnectPartialMessageTitle",
] as const;

describe("SSH 隧道遗留双语文案键移除", () => {
  it.each(["en", "zh"] as const)("为 %s 移除未引用的 SSH 隧道双语键", (language) => {
    const keyPaths = new Set(collectKeyPaths(resourceBundle(language)));
    for (const key of REMOVED_SSH_TUNNEL_BILINGUAL_KEYS) {
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

// REQ-009 / AC-016 guard: notes_search is withdrawn, so its bilingual label
// must not survive in either bundle.
const REMOVED_NOTES_SEARCH_BILINGUAL_KEYS = [
  "allowNotesSearchLabel",
] as const;

describe("notes_search 遗留双语文案键移除", () => {
  it.each(["en", "zh"] as const)("为 %s 移除未引用的 notes_search 双语键", (language) => {
    const keyPaths = new Set(collectKeyPaths(resourceBundle(language)));
    for (const key of REMOVED_NOTES_SEARCH_BILINGUAL_KEYS) {
      expect(
        keyPaths.has(key),
        `${language} 中已移除键 ${key} 不应存在`,
      ).toBe(false);
    }
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
  "aiGatewayTemplateSyncNotificationDetailProvider",
  "messageSource_ai_gateway",
] as const;

const TEMPLATE_SYNC_COUNT_KEYS = [
  "aiGatewayTemplateSyncNotificationProviderCount",
  "aiGatewayTemplateSyncNotificationAddedCount",
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

// REQ-007 / AC-007 guard: the automatic-refresh message reports additions only,
// so this unreferenced bilingual disabled-count key must be removed.
const REMOVED_TEMPLATE_SYNC_NOTIFICATION_KEYS = [
  "aiGatewayTemplateSyncNotificationDisabledCount",
] as const;

describe("服务商模板自动同步通知禁用计数键移除", () => {
  it.each(["en", "zh"] as const)(
    "为 %s 移除未引用的禁用计数键",
    async (language) => {
      await i18n.changeLanguage(language);
      for (const key of REMOVED_TEMPLATE_SYNC_NOTIFICATION_KEYS) {
        expect(i18n.t(key), `${language} 中已移除键 ${key} 应回退为键名`).toBe(
          key,
        );
      }
    },
  );
});

// ---------------------------------------------------------------------------
// Step 3: ordered upstream key-pool bilingual copy.
// RED assertions: Step 4 must add every pinned key to both dictionaries.
// ---------------------------------------------------------------------------

const PROVIDER_KEY_POOL_I18N_KEYS = [
  "aiGatewayProviderKeys",
  "aiGatewayProviderKeyAdd",
  "aiGatewayProviderKeyNameAria",
  "aiGatewayProviderKeyValueAria",
  "aiGatewayProviderKeyToggleAria",
  "aiGatewayProviderKeyMoveUpAria",
  "aiGatewayProviderKeyMoveDownAria",
  "aiGatewayProviderKeyRemoveAria",
  "aiGatewayProviderKeyReenable",
  "aiGatewayProviderKeyNameRequired",
  "aiGatewayProviderKeyValueRequired",
  "aiGatewayProviderKeyStateUsable",
  "aiGatewayProviderKeyStateDisabled",
  "aiGatewayProviderKeyStateQuota",
  "aiGatewayProviderKeyStateAuth",
  "aiGatewayProviderKeyMarkedAt",
  "aiGatewayProviderKeysSummary",
] as const;

describe("AI 网关上游密钥池国际化键", () => {
  it.each(["en", "zh"] as const)(
    "为 %s 提供全部上游密钥池真实文案",
    (language) => {
      for (const key of PROVIDER_KEY_POOL_I18N_KEYS) {
        const translation = i18n.getResource(language, "translation", key);
        expect(typeof translation, `${language}:${key} 应为字符串`).toBe(
          "string",
        );
        expect(
          (translation as string).trim(),
          `${language}:${key} 不应为空`,
        ).not.toBe("");
        expect(translation, `${language}:${key} 不应回退为键名`).not.toBe(key);
      }
    },
  );

  it.each(["en", "zh"] as const)(
    "为 %s 的标记时间与数量文案保留占位符并正常插值",
    (language) => {
      const markedAt = i18n.getResource(
        language,
        "translation",
        "aiGatewayProviderKeyMarkedAt",
      );
      expect(typeof markedAt, `${language}:markedAt 应为字符串`).toBe("string");
      expect(markedAt as string, `${language}:markedAt 应保留 {{time}}`).toContain(
        "{{time}}",
      );
      expect(
        i18n.t("aiGatewayProviderKeyMarkedAt", { time: "2026-09-25 10:00" }),
      ).toContain("2026-09-25 10:00");

      const summary = i18n.getResource(
        language,
        "translation",
        "aiGatewayProviderKeysSummary",
      );
      expect(typeof summary, `${language}:summary 应为字符串`).toBe("string");
      expect(
        summary as string,
        `${language}:summary 应保留 {{count}}`,
      ).toContain("{{count}}");
      expect(
        i18n.t("aiGatewayProviderKeysSummary", { count: 2 }),
      ).not.toContain("{{count}}");
    },
  );

  it("新增密钥池键后 en 与 zh 的键路径集合仍完全一致", () => {
    const enPaths = collectKeyPaths(resourceBundle("en"));
    const zhPaths = collectKeyPaths(resourceBundle("zh"));
    const enSet = new Set(enPaths);
    const zhSet = new Set(zhPaths);
    expect({
      onlyEn: enPaths.filter((path) => !zhSet.has(path)),
      onlyZh: zhPaths.filter((path) => !enSet.has(path)),
    }).toEqual({ onlyEn: [], onlyZh: [] });
  });
});

// ---------------------------------------------------------------------------
// 批量刷新当前筛选结果额度：新增按钮文案。
// RED assertions: Step 4 must add these exact keys to both dictionaries.
// ---------------------------------------------------------------------------

const PROVIDER_QUOTA_REFRESH_I18N_KEYS = [
  "aiGatewayProvidersRefreshQuota",
  "aiGatewayProvidersRefreshQuotaAria",
  "aiGatewayProvidersRefreshQuotaDisabled",
] as const;

const PROVIDER_QUOTA_REFRESH_COPY: Record<
  (typeof PROVIDER_QUOTA_REFRESH_I18N_KEYS)[number],
  Record<"en" | "zh", string>
> = {
  aiGatewayProvidersRefreshQuota: {
    en: "Refresh quota",
    zh: "刷新额度",
  },
  aiGatewayProvidersRefreshQuotaAria: {
    en: "Refresh quota for supported providers in the current filter results",
    zh: "刷新当前筛选结果中支持额度监控的服务商",
  },
  aiGatewayProvidersRefreshQuotaDisabled: {
    en: "No providers in the current filter results support quota refresh",
    zh: "当前筛选结果中没有可刷新额度的服务商",
  },
};

describe("AI 网关服务商批量刷新额度国际化键", () => {
  it.each(["en", "zh"] as const)(
    "为 %s 提供批量刷新额度的精确文案",
    (language) => {
      for (const key of PROVIDER_QUOTA_REFRESH_I18N_KEYS) {
        const translation = i18n.getResource(language, "translation", key);
        expect(typeof translation, `${language}:${key} 应为字符串`).toBe(
          "string",
        );
        expect(translation, `${language}:${key} 文案不匹配`).toBe(
          PROVIDER_QUOTA_REFRESH_COPY[key][language],
        );
      }
    },
  );
});

// ---------------------------------------------------------------------------
// Step 3: marked-key block heading, reason label and authentication toast copy.
// RED assertions: implementer must add these exact keys to both dictionaries.
// ---------------------------------------------------------------------------

const MARKED_KEY_SURFACING_COPY: Record<
  | "aiGatewayProviderMarkedKeys"
  | "aiGatewayProviderKeyReasonLabel"
  | "aiGatewayKeyAuthFailedToastTitle"
  | "aiGatewayKeyAuthFailedToastDescription",
  Record<"en" | "zh", string>
> = {
  aiGatewayProviderMarkedKeys: {
    en: "Marked keys",
    zh: "已标记密钥",
  },
  aiGatewayProviderKeyReasonLabel: {
    en: "Reason",
    zh: "原因",
  },
  aiGatewayKeyAuthFailedToastTitle: {
    en: "Key authentication failed",
    zh: "密钥鉴权失败",
  },
  aiGatewayKeyAuthFailedToastDescription: {
    en: "{{provider}} has {{count}} key(s) failing authentication; re-enable them in the AI Gateway.",
    zh: "{{provider}} 有 {{count}} 个密钥鉴权失败，请在 AI Gateway 中手动重新启用。",
  },
};

describe("AI 网关已标记密钥与鉴权失败通知国际化键", () => {
  it.each(["en", "zh"] as const)("为 %s 提供精确文案", (language) => {
    for (const [key, copy] of Object.entries(MARKED_KEY_SURFACING_COPY)) {
      const translation = i18n.getResource(language, "translation", key);
      expect(typeof translation, `${language}:${key} 应为字符串`).toBe(
        "string",
      );
      expect(translation, `${language}:${key} 文案不匹配`).toBe(
        copy[language],
      );
    }
  });

  it.each(["en", "zh"] as const)(
    "为 %s 的通知描述保留 provider 与 count 占位符并正常插值",
    async (language) => {
      await i18n.changeLanguage(language);
      const raw = i18n.getResource(
        language,
        "translation",
        "aiGatewayKeyAuthFailedToastDescription",
      ) as string;
      expect(raw).toContain("{{provider}}");
      expect(raw).toContain("{{count}}");

      const rendered = i18n.t("aiGatewayKeyAuthFailedToastDescription", {
        provider: "Provider Alpha",
        count: 2,
      });
      expect(rendered).toContain("Provider Alpha");
      expect(rendered).toContain("2");
      expect(rendered).not.toContain("{{");
    },
  );

  it("新增鉴权失败相关键后 en 与 zh 的键路径集合仍完全一致", () => {
    const enPaths = collectKeyPaths(resourceBundle("en"));
    const zhPaths = collectKeyPaths(resourceBundle("zh"));
    const enSet = new Set(enPaths);
    const zhSet = new Set(zhPaths);
    expect({
      onlyEn: enPaths.filter((path) => !zhSet.has(path)),
      onlyZh: zhPaths.filter((path) => !enSet.has(path)),
    }).toEqual({ onlyEn: [], onlyZh: [] });
  });
});


// ---------------------------------------------------------------------------
// Step 3: dismissible alert-badge close label and auto-disabled tooltip copy.
// RED assertions: the two keys must be added to both bundles by Step 3.
// ---------------------------------------------------------------------------

const ALERT_BADGE_I18N_COPY: Record<
  "aiGatewayAlertBadgeDismissAria" | "aiGatewayProviderAutoDisabledModelsTooltip",
  Record<"en" | "zh", string>
> = {
  aiGatewayAlertBadgeDismissAria: {
    en: "Dismiss alert",
    zh: "关闭提醒",
  },
  aiGatewayProviderAutoDisabledModelsTooltip: {
    en: "Auto-disabled mappings: {{models}}",
    zh: "已自动禁用的映射：{{models}}",
  },
};

describe("AI 网关告警标签关闭与自动禁用提示国际化键", () => {
  it.each(["en", "zh"] as const)(
    "为 %s 提供精确文案且不回退为键名",
    async (language) => {
      await i18n.changeLanguage(language);
      for (const [key, copy] of Object.entries(ALERT_BADGE_I18N_COPY)) {
        const translation = i18n.getResource(language, "translation", key);
        expect(typeof translation, `${language}:${key} 应为字符串`).toBe(
          "string",
        );
        expect(translation, `${language}:${key} 文案不匹配`).toBe(
          copy[language],
        );
        expect(translation, `${language}:${key} 不应回退为键名`).not.toBe(key);
      }
    },
  );

  it.each(["en", "zh"] as const)(
    "为 %s 的自动禁用提示保留 {{models}} 并正常插值",
    async (language) => {
      await i18n.changeLanguage(language);
      const raw = i18n.getResource(
        language,
        "translation",
        "aiGatewayProviderAutoDisabledModelsTooltip",
      ) as string;
      expect(typeof raw, `${language} 中提示应为字符串`).toBe("string");
      expect(raw).toContain("{{models}}");

      const rendered = i18n.t("aiGatewayProviderAutoDisabledModelsTooltip", {
        models: "model-two, model-three",
      });
      expect(rendered).toContain("model-two, model-three");
      expect(rendered).not.toContain("{{");
    },
  );
});

// ---------------------------------------------------------------------------
// BLE Lock tool: bilingual labels, error codes and the tray control copies.
// RED assertions: Step 3 must add every key to both dictionaries.
// ---------------------------------------------------------------------------

const BLE_LOCK_LABEL_COPY: Record<string, Record<"en" | "zh", string>> = {
  bleLock: { en: "BLE Lock", zh: "蓝牙锁" },
  bleLockDesc: {
    en: "Monitor one Bluetooth device and lock or unlock by proximity.",
    zh: "监控一个蓝牙设备，并根据距离锁定或解锁。",
  },
  bleLockLauncherDesc: {
    en: "Lock or unlock by Bluetooth proximity.",
    zh: "根据蓝牙距离锁定或解锁。",
  },
  bleLockPlatformUnavailable: {
    en: "Bluetooth lock is available only in the OneSpace macOS desktop app.",
    zh: "蓝牙锁仅在 OneSpace macOS 桌面应用中可用。",
  },
  bleLockScan: { en: "Scan for devices", zh: "扫描设备" },
  bleLockStopScan: { en: "Stop scanning", zh: "停止扫描" },
  bleLockDevices: { en: "Nearby devices", zh: "附近设备" },
  bleLockNoDevices: { en: "No devices found yet.", zh: "尚未发现设备。" },
  bleLockSelectDevice: { en: "Select", zh: "选择" },
  bleLockClearDevice: { en: "Clear device", zh: "清除设备" },
  bleLockUnlockRssi: { en: "Unlock RSSI (dBm)", zh: "解锁 RSSI (dBm)" },
  bleLockLockRssi: { en: "Lock RSSI (dBm)", zh: "锁定 RSSI (dBm)" },
  bleLockDisable: { en: "Disable", zh: "禁用" },
  bleLockLockDelay: { en: "Lock delay (seconds)", zh: "锁定延迟（秒）" },
  bleLockNoSignalTimeout: {
    en: "No-signal timeout (seconds)",
    zh: "无信号超时（秒）",
  },
  bleLockWakeOnProximity: { en: "Wake on proximity", zh: "靠近时唤醒" },
  bleLockWakeWithoutUnlocking: {
    en: "Wake without unlocking",
    zh: "只唤醒不解锁",
  },
  bleLockPauseWhileLocked: {
    en: "Pause media while locked",
    zh: "锁定时暂停媒体",
  },
  bleLockUseScreensaver: { en: "Use screensaver to lock", zh: "使用屏幕保护锁定" },
  bleLockTurnOffDisplay: { en: "Turn off display", zh: "关闭显示器" },
  bleLockLockNow: { en: "Lock now", zh: "立即锁定" },
  bleLockPassword: { en: "Login password", zh: "登录密码" },
  bleLockSetPassword: { en: "Save password", zh: "保存密码" },
  bleLockDeletePassword: { en: "Delete password", zh: "删除密码" },
  bleLockPasswordConfigured: { en: "Password saved", zh: "密码已保存" },
  bleLockRequestAccessibility: {
    en: "Request Accessibility",
    zh: "请求辅助功能权限",
  },
};

const BLE_LOCK_TRAY_COPY: Record<string, Record<"en" | "zh", string>> = {
  "tray.bleLock.controls": { en: "BLE Lock", zh: "蓝牙锁" },
  "tray.bleLock.status": { en: "Status: {{status}}", zh: "状态：{{status}}" },
  "tray.bleLock.lockNow": { en: "Lock Now", zh: "立即锁定" },
  "tray.bleLock.setDevice": { en: "Set Device", zh: "设置设备" },
  "tray.bleLock.unlockRssi": { en: "Unlock RSSI", zh: "解锁 RSSI" },
  "tray.bleLock.lockRssi": { en: "Lock RSSI", zh: "锁定 RSSI" },
  "tray.bleLock.wakeOnProximity": { en: "Wake on Proximity", zh: "靠近时唤醒" },
  "tray.bleLock.wakeWithoutUnlocking": {
    en: "Wake without Unlocking",
    zh: "只唤醒不解锁",
  },
  "tray.bleLock.pauseWhileLocked": {
    en: "Pause while Locked",
    zh: "锁定时暂停",
  },
  "tray.bleLock.useScreensaver": {
    en: "Use Screensaver",
    zh: "使用屏幕保护",
  },
  "tray.bleLock.turnOffDisplay": { en: "Turn Off Display", zh: "关闭显示器" },
  "tray.bleLock.disable": { en: "Disable", zh: "禁用" },
};

const BLE_LOCK_ERROR_CODES = [
  "platform_unavailable",
  "bluetooth_powered_off",
  "bluetooth_unauthorized",
  "bluetooth_unsupported",
  "device_not_selected",
  "device_unavailable",
  "invalid_config",
  "keychain_unavailable",
  "password_not_configured",
  "accessibility_required",
  "system_sleep",
  "native_action_unavailable",
  "media_control_unavailable",
] as const;

describe("蓝牙锁国际化键", () => {
  it.each(["en", "zh"] as const)(
    "为 %s 提供工具界面精确文案",
    async (language) => {
      await i18n.changeLanguage(language);
      for (const [key, copy] of Object.entries(BLE_LOCK_LABEL_COPY)) {
        expect(i18n.t(key), `${language}:${key} 文案不匹配`).toBe(
          copy[language],
        );
      }
    },
  );

  it.each(["en", "zh"] as const)(
    "为 %s 提供托盘控制精确文案",
    async (language) => {
      await i18n.changeLanguage(language);
      for (const [key, copy] of Object.entries(BLE_LOCK_TRAY_COPY)) {
        expect(i18n.t(key), `${language}:${key} 文案不匹配`).toBe(
          copy[language],
        );
      }
    },
  );

  it.each(["en", "zh"] as const)(
    "为 %s 的托盘状态行保留 {{status}} 并正常插值",
    async (language) => {
      await i18n.changeLanguage(language);
      const raw = resourceBundle(language)["tray.bleLock.status"];
      expect(typeof raw, `${language}:tray.bleLock.status 应为字符串`).toBe(
        "string",
      );
      expect(raw as string).toContain("{{status}}");
      expect(i18n.t("tray.bleLock.status", { status: "near" })).toContain("near");
      expect(i18n.t("tray.bleLock.status", { status: "near" })).not.toContain(
        "{{",
      );
    },
  );

  it.each(["en", "zh"] as const)(
    "为 %s 提供全部稳定错误码的真实文案",
    async (language) => {
      await i18n.changeLanguage(language);
      for (const code of BLE_LOCK_ERROR_CODES) {
        const key = `bleLockError_${code}`;
        const value = i18n.t(key);
        expect(value, `${language}:${key} 应回退为键名`).not.toBe(key);
        expect(value.trim(), `${language}:${key} 不应为空`).not.toBe("");
      }
    },
  );

  it("新增蓝牙锁键后 en 与 zh 的键路径集合仍完全一致", () => {
    const enPaths = collectKeyPaths(resourceBundle("en"));
    const zhPaths = collectKeyPaths(resourceBundle("zh"));
    const enSet = new Set(enPaths);
    const zhSet = new Set(zhPaths);
    expect({
      onlyEn: enPaths.filter((path) => !zhSet.has(path)),
      onlyZh: zhPaths.filter((path) => !enSet.has(path)),
    }).toEqual({ onlyEn: [], onlyZh: [] });
  });
});

// ---------------------------------------------------------------------------
// Step 2: provider-template binding, strategy and broken-binding bilingual
// copy (REQ-008 / AC-023). The new keys must exist in both dictionaries and in
// en_keys.txt / zh_keys.txt with parity.
// ---------------------------------------------------------------------------

const TEMPLATE_POLICY_BINDING_I18N_KEYS = [
  "aiGatewayTemplateSyncStrategy",
  "aiGatewayTemplateQuotaStrategy",
  "aiGatewayTemplateErrorStrategy",
  "aiGatewayTemplateSyncStrategyGeneric",
  "aiGatewayTemplateSyncStrategyOpencodeGo",
  "aiGatewayTemplateQuotaStrategyNone",
  "aiGatewayTemplateQuotaStrategyCommandcode",
  "aiGatewayTemplateQuotaStrategyOpencodeGo",
  "aiGatewayTemplateErrorStrategyGeneric",
  "aiGatewayProviderTemplateBinding",
  "aiGatewayProviderTemplateUnbound",
  "aiGatewayProviderTemplateBrokenBinding",
  "aiGatewayProviderEffectiveStrategy",
] as const;

function readKeyListFile(fileName: string): Set<string> {
  const contents = readFileSync(resolve(process.cwd(), fileName), "utf8");
  return new Set(
    contents
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line !== ""),
  );
}

describe("AI 网关服务商模板策略与绑定国际化键", () => {
  it.each(["en", "zh"] as const)(
    "为 %s 提供全部模板策略与绑定真实文案",
    (language) => {
      for (const key of TEMPLATE_POLICY_BINDING_I18N_KEYS) {
        const translation = i18n.getResource(language, "translation", key);
        expect(typeof translation, `${language}:${key} 应为字符串`).toBe(
          "string",
        );
        expect(
          (translation as string).trim(),
          `${language}:${key} 不应为空`,
        ).not.toBe("");
        expect(translation, `${language}:${key} 不应回退为键名`).not.toBe(key);
      }
    },
  );

  it("新增策略与绑定键后 en 与 zh 的键路径集合仍完全一致", () => {
    const enPaths = collectKeyPaths(resourceBundle("en"));
    const zhPaths = collectKeyPaths(resourceBundle("zh"));
    const enSet = new Set(enPaths);
    const zhSet = new Set(zhPaths);
    expect({
      onlyEn: enPaths.filter((path) => !zhSet.has(path)),
      onlyZh: zhPaths.filter((path) => !enSet.has(path)),
    }).toEqual({ onlyEn: [], onlyZh: [] });
  });

  it("en_keys.txt 与 zh_keys.txt 收录全部新增键且键集合保持一致", () => {
    const enKeys = readKeyListFile("en_keys.txt");
    const zhKeys = readKeyListFile("zh_keys.txt");

    expect({
      onlyEn: [...enKeys].filter((key) => !zhKeys.has(key)),
      onlyZh: [...zhKeys].filter((key) => !enKeys.has(key)),
    }).toEqual({ onlyEn: [], onlyZh: [] });

    for (const key of TEMPLATE_POLICY_BINDING_I18N_KEYS) {
      expect(enKeys.has(key), `en_keys.txt 缺少 ${key}`).toBe(true);
      expect(zhKeys.has(key), `zh_keys.txt 缺少 ${key}`).toBe(true);
    }
  });
});

describe("AI 网关页面副标题国际化文案", () => {
  it("en 提供根据实际功能逻辑更新后的 AI 网关副标题文案", () => {
    const translation = i18n.getResource(
      "en",
      "translation",
      "aiGatewayWorkspaceDesc",
    );
    expect(translation).toBe(
      "Manage upstream providers and key pools with OpenAI / Anthropic dual-protocol relay, intelligent routing, and failover, syncing seamlessly to OpenCode, Codex, and Claude Code.",
    );
  });

  it("zh 提供根据实际功能逻辑更新后的 AI 网关副标题文案", () => {
    const translation = i18n.getResource(
      "zh",
      "translation",
      "aiGatewayWorkspaceDesc",
    );
    expect(translation).toBe(
      "统一管理多上游服务商与密钥池，提供 OpenAI / Anthropic 双协议转发、智能路由与故障转移，并一键同步到 OpenCode / Codex / Claude Code 等终端。",
    );
  });
});
