import { createElement, type ReactNode } from "react";
import { act, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ToastProvider";
import i18n from "@/i18n";
import {
  notifyTemplateAutoRefreshIntervalChanged,
  type GatewayConfig,
  type GatewayModelMapping,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateView,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import type { MessageCreateInput } from "@/lib/messages";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import {
  recordMessageMock,
  resetMessageMocks,
  safeRecordMessageMock,
} from "@/test/mocks/messages";
import {
  clearTemplateAutoRefreshFailures,
  clearTemplateSyncInFlight,
  isTemplateSyncInFlight,
  setTemplateAutoRefreshFailure,
  setTemplateSyncInFlight,
  useTemplateAutoRefresh,
  useTemplateAutoRefreshFailures,
} from "./useTemplateAutoRefresh";

const AUTO_REFRESH_GET_COMMAND = "ai_gateway_template_auto_refresh_get";
const PROVIDER_TEMPLATES_COMMAND = "ai_gateway_provider_templates";
const SYNC_PROVIDER_TEMPLATE_COMMAND = "ai_gateway_sync_provider_template";
const GET_CONFIG_COMMAND = "ai_gateway_get_config";

function makeTemplate(
  id: string,
  modelsUrl: string | null,
): GatewayProviderTemplate {
  return {
    id,
    name: id,
    description: "",
    base_url: "https://example.test/v1",
    protocol: "chat_completions",
    source: "",
    models_url: modelsUrl,
    models: [],
  };
}

function makeView(id: string, modelsUrl: string | null): GatewayProviderTemplateView {
  return {
    template: makeTemplate(id, modelsUrl),
    synced_at: null,
    source: "",
    from_snapshot: false,
  };
}

type SyncHandler = (
  templateId: string,
  index: number,
) => unknown | Promise<unknown>;

function makeMapping(
  overrides: Partial<GatewayModelMapping> = {},
): GatewayModelMapping {
  return {
    local_model: "m",
    upstream_model: "m",
    enabled: true,
    ...overrides,
  };
}

function makeProvider(
  overrides: Partial<GatewayUpstreamProvider> = {},
): GatewayUpstreamProvider {
  return {
    id: "p1",
    name: "Provider One",
    base_url: "https://upstream.example/v1",
    api_key: "********",
    default_model: null,
    mappings: [],
    enabled: true,
    auto_disabled: false,
    disabled_reason: null,
    disabled_at: null,
    consecutive_failures: 0,
    last_error_at: null,
    ...overrides,
  };
}

function makeConfig(providers: GatewayUpstreamProvider[] = []): GatewayConfig {
  return {
    enabled: true,
    port: 17688,
    providers,
    keys: [],
    default_key_id: null,
    terminal_syncs: [],
  };
}

/** One `ai_gateway_get_config` result; an `Error` makes that call reject. */
type ConfigStep = GatewayConfig | Error;

function installInvoke(options: {
  interval: unknown;
  templates: GatewayProviderTemplateView[];
  sync?: SyncHandler;
  configs?: ConfigStep[];
  config?: GatewayConfig;
}): void {
  let syncIndex = 0;
  let configIndex = 0;
  invokeMock.mockImplementation(
    async (command: string, args?: Record<string, unknown>) => {
      switch (command) {
        case AUTO_REFRESH_GET_COMMAND:
          return options.interval;
        case PROVIDER_TEMPLATES_COMMAND:
          return options.templates;
        case GET_CONFIG_COMMAND: {
          const index = configIndex;
          configIndex += 1;
          const step = options.configs?.[index];
          if (step instanceof Error) throw step;
          if (step !== undefined) return step;
          return options.config ?? makeConfig();
        }
        case SYNC_PROVIDER_TEMPLATE_COMMAND: {
          const index = syncIndex;
          syncIndex += 1;
          return options.sync
            ? await options.sync(String(args?.templateId), index)
            : undefined;
        }
        default:
          return undefined;
      }
    },
  );
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function ToastWrapper({ children }: { children: ReactNode }) {
  return createElement(ToastProvider, null, children);
}

function mountAutoRefresh() {
  return renderHook(
    () => {
      useTemplateAutoRefresh();
      return useTemplateAutoRefreshFailures();
    },
    { wrapper: ToastWrapper },
  );
}

async function settle(ms = 0): Promise<void> {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

/** Inputs passed to the intercepted `safeRecordMessage` public boundary. */
function recordedMessages(): MessageCreateInput[] {
  return safeRecordMessageMock.mock.calls.map(
    (call) => (call as unknown[])[0] as MessageCreateInput,
  );
}

/** Switch the shared i18n instance even while fake timers are installed. */
async function setLanguage(language: "en" | "zh"): Promise<void> {
  const changing = i18n.changeLanguage(language);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
  await changing;
}

function tTitle(template: string): string {
  return i18n.t("aiGatewayTemplateSyncNotificationTitle", { template });
}

function tProviderCount(count: number): string {
  return i18n.t("aiGatewayTemplateSyncNotificationProviderCount", { count });
}

function tAddedCount(count: number): string {
  return i18n.t("aiGatewayTemplateSyncNotificationAddedCount", { count });
}

function tDisabledCount(count: number): string {
  return i18n.t("aiGatewayTemplateSyncNotificationDisabledCount", { count });
}

function tDetailProvider(provider: string, models: string): string {
  return i18n.t("aiGatewayTemplateSyncNotificationDetailProvider", {
    provider,
    models,
  });
}

function syncIds(): string[] {
  return invokeMock.mock.calls
    .filter(([command]) => command === SYNC_PROVIDER_TEMPLATE_COMMAND)
    .map(([, args]) => String((args as Record<string, unknown>)?.templateId));
}

function batchCount(): number {
  return invokeMock.mock.calls.filter(
    ([command]) => command === PROVIDER_TEMPLATES_COMMAND,
  ).length;
}

function autoRefreshReads(): number {
  return invokeMock.mock.calls.filter(
    ([command]) => command === AUTO_REFRESH_GET_COMMAND,
  ).length;
}

describe("useTemplateAutoRefresh 模板自动刷新调度", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    resetTauriMocks();
    resetMessageMocks();
    clearTemplateSyncInFlight();
    clearTemplateAutoRefreshFailures();
    window.localStorage.clear();
  });

  afterEach(() => {
    clearTemplateSyncInFlight();
    clearTemplateAutoRefreshFailures();
    vi.useRealTimers();
  });

  it("schedulesTheFirstBatchOnlyAfterTheConfiguredIntervalAndSkipsBlankModelsUrl", async () => {
    installInvoke({
      interval: 10,
      templates: [
        makeView("t1", "https://one.test/models"),
        makeView("t-blank", ""),
        makeView("t-space", "   "),
        makeView("t2", "https://two.test/models"),
      ],
      sync: () => ({}),
    });

    mountAutoRefresh();
    await settle();

    // 挂载时只读取间隔，不立即执行批次
    expect(autoRefreshReads()).toBe(1);
    expect(batchCount()).toBe(0);
    expect(syncIds()).toEqual([]);

    await settle(9 * 60_000);
    expect(batchCount()).toBe(0);
    expect(syncIds()).toEqual([]);

    await settle(60_000);
    expect(batchCount()).toBe(1);
    // 仅同步 models_url 非空的模板，且保持列表顺序
    expect(syncIds()).toEqual(["t1", "t2"]);
  });

  it("intervalZeroNeverRunsABatch", async () => {
    installInvoke({
      interval: 0,
      templates: [makeView("t1", "https://one.test/models")],
      sync: () => ({}),
    });

    mountAutoRefresh();
    await settle();
    await settle(60 * 60_000);

    expect(batchCount()).toBe(0);
    expect(syncIds()).toEqual([]);
  });

  it.each([
    ["undefined", undefined],
    ["null", null],
    ["NaN", Number.NaN],
    ["non-numeric string", "not-a-number"],
  ])("tolerates a %s interval result without throwing or scheduling", async (_label, interval) => {
    installInvoke({
      interval,
      templates: [makeView("t1", "https://one.test/models")],
      sync: () => ({}),
    });

    expect(() => mountAutoRefresh()).not.toThrow();
    await settle();
    await settle(60 * 60_000);

    expect(batchCount()).toBe(0);
    expect(syncIds()).toEqual([]);
  });

  it("skipsATickWhileABatchIsInFlight", async () => {
    const first = deferred<unknown>();
    installInvoke({
      interval: 10,
      templates: [
        makeView("t1", "https://one.test/models"),
        makeView("t2", "https://two.test/models"),
      ],
      sync: (_templateId, index) => (index === 0 ? first.promise : {}),
    });

    mountAutoRefresh();
    await settle();

    await settle(10 * 60_000);
    expect(batchCount()).toBe(1);
    expect(syncIds()).toEqual(["t1"]);

    // 批次仍在进行中，下一个 tick 必须被跳过
    await settle(10 * 60_000);
    expect(batchCount()).toBe(1);
    expect(syncIds()).toEqual(["t1"]);

    await act(async () => {
      first.resolve({});
      await first.promise;
    });
    await settle();
    // 解析后批次继续，第二个模板被同步
    expect(syncIds()).toEqual(["t1", "t2"]);

    // 之后的 tick 可以运行下一个批次，每个模板每批次只同步一次
    await settle(10 * 60_000);
    expect(batchCount()).toBe(2);
    expect(syncIds()).toEqual(["t1", "t2", "t1", "t2"]);
  });

  it("isolatesOneTemplateFailureAndPushesNoToast", async () => {
    installInvoke({
      interval: 10,
      templates: [
        makeView("t1", "https://one.test/models"),
        makeView("t2", "https://two.test/models"),
      ],
      sync: (templateId) =>
        templateId === "t1"
          ? Promise.reject(new Error("network down"))
          : {},
    });

    const { result } = mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    // 失败模板之后，健康模板仍在同一批次内被同步
    expect(syncIds()).toEqual(["t1", "t2"]);
    expect(Object.keys(result.current)).toEqual(["t1"]);
    expect(result.current.t1).toContain("network down");
    expect(result.current.t2).toBeUndefined();

    // 调度器绝不弹出 toast（渲染失败文案的是卡片测试，不在本 hook 测试中）
    expect(document.body.textContent ?? "").not.toContain("Auto refresh failed");
    expect(document.body.textContent ?? "").not.toContain("network down");
    expect(screen.queryByText("Action failed")).not.toBeInTheDocument();
  });

  it("failurePathPerformsNoPersistenceBeyondTheReadAndSyncCommands", async () => {
    installInvoke({
      interval: 10,
      templates: [
        makeView("t1", "https://one.test/models"),
        makeView("t2", "https://two.test/models"),
      ],
      sync: (templateId) =>
        templateId === "t1"
          ? Promise.reject(new Error("network down"))
          : {},
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    expect(syncIds()).toEqual(["t1", "t2"]);

    // 失败批次只能读取（含只读的配置读取）与同步，绝不能触发配置/服务商/模板的持久化命令。
    const commands = new Set(
      invokeMock.mock.calls.map(([command]) => String(command)),
    );
    expect([...commands].sort()).toEqual(
      [
        AUTO_REFRESH_GET_COMMAND,
        PROVIDER_TEMPLATES_COMMAND,
        GET_CONFIG_COMMAND,
        SYNC_PROVIDER_TEMPLATE_COMMAND,
      ].sort(),
    );

    // 失败状态仅存在于内存，不写入 localStorage。
    expect(window.localStorage.length).toBe(0);
  });

  it("clearsFailuresOnTheNextSuccessfulBatchAndSupportsManualClear", async () => {
    let syncIndex = 0;
    installInvoke({
      interval: 10,
      templates: [makeView("t1", "https://one.test/models")],
      sync: () => {
        syncIndex += 1;
        return syncIndex === 1 ? Promise.reject(new Error("first failure")) : {};
      },
    });

    const { result, unmount } = mountAutoRefresh();
    await settle();

    await settle(10 * 60_000);
    expect(result.current.t1).toContain("first failure");

    // 下一次成功批次自动清除该模板的失败
    await settle(10 * 60_000);
    expect(result.current).toEqual({});

    // 手动记录后再以 null 清除
    act(() => setTemplateAutoRefreshFailure("t1", "manual reason"));
    expect(result.current).toEqual({ t1: "manual reason" });
    act(() => setTemplateAutoRefreshFailure("t1", null));
    expect(result.current).toEqual({});

    // 已清除的内存状态在新一次挂载后不会复活
    unmount();
    const fresh = mountAutoRefresh();
    await settle();
    expect(fresh.result.current).toEqual({});
  });

  it("defersToTheManualSyncInFlightRegistry", async () => {
    installInvoke({
      interval: 10,
      templates: [
        makeView("t1", "https://one.test/models"),
        makeView("t2", "https://two.test/models"),
      ],
      sync: () => ({}),
    });

    mountAutoRefresh();
    await settle();

    act(() => setTemplateSyncInFlight("t1", true));
    expect(isTemplateSyncInFlight("t1")).toBe(true);

    await settle(10 * 60_000);
    // t1 正被手动同步，自动批次必须跳过它但仍同步 t2
    expect(syncIds()).toEqual(["t2"]);

    act(() => setTemplateSyncInFlight("t1", false));
    expect(isTemplateSyncInFlight("t1")).toBe(false);

    await settle(10 * 60_000);
    expect(syncIds()).toEqual(["t2", "t1", "t2"]);
  });

  it("recomputesTheTimerWhenThePersistedIntervalChanges", async () => {
    const templates = [makeView("t1", "https://one.test/models")];
    installInvoke({ interval: 10, templates, sync: () => ({}) });

    mountAutoRefresh();
    await settle();

    // 持久化改为 0 并通知后，不再有任何触发
    installInvoke({ interval: 0, templates, sync: () => ({}) });
    act(() => notifyTemplateAutoRefreshIntervalChanged());
    await settle();
    await settle(30 * 60_000);
    expect(batchCount()).toBe(0);

    // 改为 20 并通知：下一次触发应在通知后 20 分钟（不是 10 分钟，也不是立即）
    installInvoke({ interval: 20, templates, sync: () => ({}) });
    act(() => notifyTemplateAutoRefreshIntervalChanged());
    await settle();
    expect(batchCount()).toBe(0);

    await settle(19 * 60_000);
    expect(batchCount()).toBe(0);

    await settle(60_000);
    expect(batchCount()).toBe(1);
    expect(syncIds()).toEqual(["t1"]);
  });

  // -------------------------------------------------------------------------
  // 自动刷新同步通知（变更门控）
  // 全部断言只观察公开边界：mock 的 Tauri invoke 命令流与 mock 的
  // @/lib/messages safeRecordMessage。
  // -------------------------------------------------------------------------

  it("AC-001/AC-007 新增映射创建一条通知且消息信封完整", async () => {
    await setLanguage("en");
    const before = makeConfig([
      makeProvider({
        id: "p1",
        name: "Zen Upstream",
        template_id: "opencode-zen",
        mappings: [],
      }),
    ]);
    const after = makeConfig([
      makeProvider({
        id: "p1",
        name: "Zen Upstream",
        template_id: "opencode-zen",
        mappings: [
          makeMapping({ local_model: "m", upstream_model: "m", enabled: true }),
        ],
      }),
    ]);
    installInvoke({
      interval: 10,
      templates: [makeView("opencode-zen", "https://zen.test/models")],
      sync: () => ({}),
      configs: [before, after],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    expect(syncIds()).toEqual(["opencode-zen"]);

    const messages = recordedMessages();
    expect(messages).toHaveLength(1);
    const [message] = messages;
    expect(message.target?.tab).toBe("ai-gateway");
    expect(message.source).toBe("ai_gateway");
    expect(message.category).toBe("template_sync");
    expect(message.severity).toBe("info");
    expect(message.dedupe_key).toBeUndefined();
    expect(message.title).toBe(tTitle("opencode-zen"));
    expect(message.detail).toContain("m");
    // 只使用 safeRecordMessage 边界，不直接落到 recordMessage。
    expect(recordMessageMock).not.toHaveBeenCalled();
    // 摘要必须与「服务商数 + 新增数」完全一致，证明禁用子句被省略。
    expect(message.summary).toBe(
      [tProviderCount(1), tAddedCount(1)].join("; "),
    );
  });

  it("AC-007 通知消息信封包含固定 source/category/severity/target 且无 dedupe_key", async () => {
    await setLanguage("en");
    const before = makeConfig([
      makeProvider({ id: "p1", name: "Zen Upstream", template_id: "opencode-zen" }),
    ]);
    const after = makeConfig([
      makeProvider({
        id: "p1",
        name: "Zen Upstream",
        template_id: "opencode-zen",
        mappings: [makeMapping({ local_model: "m", upstream_model: "m" })],
      }),
    ]);
    installInvoke({
      interval: 10,
      templates: [makeView("opencode-zen", "https://zen.test/models")],
      sync: () => ({}),
      configs: [before, after],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    const messages = recordedMessages();
    expect(messages).toHaveLength(1);
    expect(messages[0].source).toBe("ai_gateway");
    expect(messages[0].category).toBe("template_sync");
    expect(messages[0].severity).toBe("info");
    expect(messages[0].target).toMatchObject({ tab: "ai-gateway" });
    expect(messages[0].dedupe_key).toBeUndefined();
  });

  it("AC-002 失效映射转为禁用的通知摘要只含禁用子句", async () => {
    await setLanguage("en");
    const before = makeConfig([
      makeProvider({
        id: "p1",
        name: "Zen Upstream",
        template_id: "opencode-zen",
        mappings: [makeMapping({ local_model: "m", upstream_model: "m", enabled: true })],
      }),
    ]);
    const after = makeConfig([
      makeProvider({
        id: "p1",
        name: "Zen Upstream",
        template_id: "opencode-zen",
        mappings: [makeMapping({ local_model: "m", upstream_model: "m", enabled: false })],
      }),
    ]);
    installInvoke({
      interval: 10,
      templates: [makeView("opencode-zen", "https://zen.test/models")],
      sync: () => ({}),
      configs: [before, after],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    const messages = recordedMessages();
    expect(messages).toHaveLength(1);
    expect(messages[0].detail).toContain("m");
    // 摘要必须与「服务商数 + 禁用数」完全一致，证明新增子句被省略。
    expect(messages[0].summary).toBe(
      [tProviderCount(1), tDisabledCount(1)].join("; "),
    );
  });

  it("AC-003a 仅显示名/协议/服务商名称变化不创建通知", async () => {
    await setLanguage("en");
    const before = makeConfig([
      makeProvider({
        id: "p1",
        name: "Zen Upstream",
        template_id: "opencode-zen",
        mappings: [
          makeMapping({
            local_model: "m",
            upstream_model: "m",
            enabled: true,
            display_name: "Old Name",
            protocol: null,
          }),
        ],
      }),
    ]);
    const after = makeConfig([
      makeProvider({
        id: "p1",
        name: "Renamed Upstream",
        template_id: "opencode-zen",
        mappings: [
          makeMapping({
            local_model: "m",
            upstream_model: "m",
            enabled: true,
            display_name: "New Name",
            protocol: "responses",
          }),
        ],
      }),
    ]);
    installInvoke({
      interval: 10,
      templates: [makeView("opencode-zen", "https://zen.test/models")],
      sync: () => ({}),
      configs: [before, after],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    expect(syncIds()).toEqual(["opencode-zen"]);
    expect(recordedMessages()).toHaveLength(0);
  });

  it("AC-003b 未绑定该模板的服务商变化不创建通知", async () => {
    await setLanguage("en");
    const before = makeConfig([
      makeProvider({ id: "p-other", name: "Other", template_id: "other-template" }),
      makeProvider({ id: "p-manual", name: "Manual" }),
    ]);
    const after = makeConfig([
      makeProvider({
        id: "p-other",
        name: "Other",
        template_id: "other-template",
        mappings: [makeMapping({ local_model: "x", upstream_model: "x" })],
      }),
      makeProvider({
        id: "p-manual",
        name: "Manual",
        mappings: [makeMapping({ local_model: "y", upstream_model: "y" })],
      }),
    ]);
    installInvoke({
      interval: 10,
      templates: [makeView("opencode-zen", "https://zen.test/models")],
      sync: () => ({}),
      configs: [before, after],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    expect(syncIds()).toEqual(["opencode-zen"]);
    expect(recordedMessages()).toHaveLength(0);
  });

  it("AC-003c 已禁用映射与 ignored_models 跳过项不创建通知", async () => {
    await setLanguage("en");
    const before = makeConfig([
      makeProvider({
        id: "p-disabled",
        name: "Already Disabled",
        template_id: "opencode-zen",
        mappings: [makeMapping({ local_model: "m", upstream_model: "m", enabled: false })],
      }),
      makeProvider({
        id: "p-ignoring",
        name: "Ignoring",
        template_id: "opencode-zen",
        ignored_models: ["skipped"],
        mappings: [],
      }),
    ]);
    const after = makeConfig([
      makeProvider({
        id: "p-disabled",
        name: "Already Disabled",
        template_id: "opencode-zen",
        mappings: [makeMapping({ local_model: "m", upstream_model: "m", enabled: false })],
      }),
      makeProvider({
        id: "p-ignoring",
        name: "Ignoring",
        template_id: "opencode-zen",
        ignored_models: ["skipped"],
        mappings: [],
      }),
    ]);
    installInvoke({
      interval: 10,
      templates: [makeView("opencode-zen", "https://zen.test/models")],
      sync: () => ({}),
      configs: [before, after],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    expect(syncIds()).toEqual(["opencode-zen"]);
    expect(recordedMessages()).toHaveLength(0);
  });

  it("AC-004 同一模板的多个受影响服务商聚合成一条通知", async () => {
    await setLanguage("en");
    const p1 = makeProvider({ id: "p1", name: "P1", template_id: "T1" });
    const p2 = makeProvider({ id: "p2", name: "P2", template_id: "T1" });
    const t2Provider = makeProvider({
      id: "p3",
      name: "P3",
      template_id: "T2",
      mappings: [makeMapping({ local_model: "x", upstream_model: "x" })],
    });
    const c0 = makeConfig([p1, p2, t2Provider]);
    const c1 = makeConfig([
      { ...p1, mappings: [makeMapping({ local_model: "a", upstream_model: "a" })] },
      { ...p2, mappings: [makeMapping({ local_model: "b", upstream_model: "b" })] },
      t2Provider,
    ]);
    const c2 = makeConfig([
      { ...p1, mappings: [makeMapping({ local_model: "a", upstream_model: "a" })] },
      { ...p2, mappings: [makeMapping({ local_model: "b", upstream_model: "b" })] },
      t2Provider,
    ]);
    installInvoke({
      interval: 10,
      templates: [
        makeView("T1", "https://t1.test/models"),
        makeView("T2", "https://t2.test/models"),
      ],
      sync: () => ({}),
      configs: [c0, c1, c2],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    expect(syncIds()).toEqual(["T1", "T2"]);
    const messages = recordedMessages();
    expect(messages).toHaveLength(1);
    expect(messages[0].title).toContain("T1");
    expect(messages[0].title).not.toContain("T2");
    expect(messages[0].summary).toContain(tProviderCount(2));
  });

  it("AC-005/AC-006 标题、计数摘要与逐服务商明细（含空白 local_model 回退）", async () => {
    await setLanguage("en");
    const before = makeConfig([
      makeProvider({
        id: "p1",
        name: "Zen Upstream",
        template_id: "opencode-zen",
        mappings: [
          makeMapping({ local_model: "keep", upstream_model: "keep", enabled: true }),
          makeMapping({ local_model: "old", upstream_model: "old", enabled: true }),
        ],
      }),
    ]);
    const after = makeConfig([
      makeProvider({
        id: "p1",
        name: "Zen Upstream",
        template_id: "opencode-zen",
        mappings: [
          makeMapping({ local_model: "keep", upstream_model: "keep", enabled: true }),
          makeMapping({ local_model: "new-a", upstream_model: "new-a", enabled: true }),
          makeMapping({ local_model: "   ", upstream_model: "remote-b", enabled: true }),
          makeMapping({ local_model: "old", upstream_model: "old", enabled: false }),
        ],
      }),
    ]);
    installInvoke({
      interval: 10,
      templates: [makeView("opencode-zen", "https://zen.test/models")],
      sync: () => ({}),
      configs: [before, after],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    const messages = recordedMessages();
    expect(messages).toHaveLength(1);
    const [message] = messages;
    expect(message.title).toContain("opencode-zen");
    expect(message.summary).toContain(tProviderCount(1));
    expect(message.summary).toContain(tAddedCount(2));
    expect(message.summary).toContain(tDisabledCount(1));
    expect(message.detail).toContain("Zen Upstream");
    expect(message.detail).toContain("new-a");
    expect(message.detail).toContain("remote-b");
    expect(message.detail).toContain("old");
  });

  it("AC-006 明细优先使用非空 local_model 而非 upstream_model", async () => {
    await setLanguage("en");
    // 唯一的差异来源：一个新增映射，其 local_model 与 upstream_model 不同。
    // 若实现总是输出 upstream_model，则 detail 会包含 "remote-alias"，本用例必须失败。
    const before = makeConfig([
      makeProvider({
        id: "p1",
        name: "Zen Upstream",
        template_id: "opencode-zen",
        mappings: [],
      }),
    ]);
    const after = makeConfig([
      makeProvider({
        id: "p1",
        name: "Zen Upstream",
        template_id: "opencode-zen",
        mappings: [
          makeMapping({
            local_model: "local-alias",
            upstream_model: "remote-alias",
            enabled: true,
          }),
        ],
      }),
    ]);
    installInvoke({
      interval: 10,
      templates: [makeView("opencode-zen", "https://zen.test/models")],
      sync: () => ({}),
      configs: [before, after],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    const messages = recordedMessages();
    expect(messages).toHaveLength(1);
    expect(messages[0].detail).toContain("local-alias");
    expect(messages[0].detail).not.toContain("remote-alias");
  });

  it("AC-008 两个连续周期的真实变更各自创建独立通知且无 dedupe_key", async () => {
    await setLanguage("en");
    const provider = makeProvider({
      id: "p1",
      name: "Zen Upstream",
      template_id: "opencode-zen",
      mappings: [],
    });
    const c0 = makeConfig([provider]);
    const c1 = makeConfig([
      {
        ...provider,
        mappings: [makeMapping({ local_model: "a", upstream_model: "a" })],
      },
    ]);
    const c2 = makeConfig([
      {
        ...provider,
        mappings: [
          makeMapping({ local_model: "a", upstream_model: "a" }),
          makeMapping({ local_model: "b", upstream_model: "b" }),
        ],
      },
    ]);
    installInvoke({
      interval: 10,
      templates: [makeView("opencode-zen", "https://zen.test/models")],
      sync: () => ({}),
      configs: [c0, c1, c1, c2],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);
    expect(recordedMessages()).toHaveLength(1);

    await settle(10 * 60_000);
    const messages = recordedMessages();
    expect(messages).toHaveLength(2);
    expect(messages[0].dedupe_key).toBeUndefined();
    expect(messages[1].dedupe_key).toBeUndefined();
  });

  it("AC-010a 单个模板同步失败不阻塞兄弟模板的同步与通知", async () => {
    await setLanguage("en");
    const t2Before = makeProvider({
      id: "p2",
      name: "T2 Provider",
      template_id: "T2",
      mappings: [],
    });
    const t2After = {
      ...t2Before,
      mappings: [makeMapping({ local_model: "b", upstream_model: "b" })],
    };
    installInvoke({
      interval: 10,
      templates: [
        makeView("T1", "https://t1.test/models"),
        makeView("T2", "https://t2.test/models"),
      ],
      sync: (templateId) =>
        templateId === "T1"
          ? Promise.reject(new Error("t1 sync down"))
          : {},
      configs: [makeConfig([t2Before]), makeConfig([t2After])],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    expect(syncIds()).toEqual(["T1", "T2"]);
    const messages = recordedMessages();
    expect(messages).toHaveLength(1);
    expect(messages[0].title).toContain("T2");
  });

  it("AC-010b 同步后配置读取失败只跳过该模板的通知", async () => {
    await setLanguage("en");
    const t2Before = makeProvider({
      id: "p2",
      name: "T2 Provider",
      template_id: "T2",
      mappings: [],
    });
    const t2After = {
      ...t2Before,
      mappings: [makeMapping({ local_model: "b", upstream_model: "b" })],
    };
    installInvoke({
      interval: 10,
      templates: [
        makeView("T1", "https://t1.test/models"),
        makeView("T2", "https://t2.test/models"),
      ],
      sync: () => ({}),
      configs: [makeConfig([t2Before]), new Error("post sync read failed"), makeConfig([t2After])],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    expect(syncIds()).toEqual(["T1", "T2"]);
    const messages = recordedMessages();
    expect(messages).toHaveLength(1);
    expect(messages[0].title).toContain("T2");
  });

  it("AC-010c 批次前配置读取失败时不产生任何通知但仍同步全部模板", async () => {
    await setLanguage("en");
    installInvoke({
      interval: 10,
      templates: [
        makeView("T1", "https://t1.test/models"),
        makeView("T2", "https://t2.test/models"),
      ],
      sync: () => ({}),
      configs: [new Error("pre batch read failed")],
    });

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    expect(syncIds()).toEqual(["T1", "T2"]);
    expect(recordedMessages()).toHaveLength(0);
  });

  it("AC-010d 通知写入失败不影响兄弟模板的同步与通知", async () => {
    await setLanguage("en");
    const p1 = makeProvider({ id: "p1", name: "T1 Provider", template_id: "T1" });
    const p2 = makeProvider({ id: "p2", name: "T2 Provider", template_id: "T2" });
    const c0 = makeConfig([p1, p2]);
    const c1 = makeConfig([
      { ...p1, mappings: [makeMapping({ local_model: "a", upstream_model: "a" })] },
      p2,
    ]);
    const c2 = makeConfig([
      { ...p1, mappings: [makeMapping({ local_model: "a", upstream_model: "a" })] },
      { ...p2, mappings: [makeMapping({ local_model: "b", upstream_model: "b" })] },
    ]);
    installInvoke({
      interval: 10,
      templates: [
        makeView("T1", "https://t1.test/models"),
        makeView("T2", "https://t2.test/models"),
      ],
      sync: () => ({}),
      configs: [c0, c1, c2],
    });
    safeRecordMessageMock.mockRejectedValueOnce(new Error("message store down"));

    mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);

    expect(syncIds()).toEqual(["T1", "T2"]);
    expect(safeRecordMessageMock).toHaveBeenCalledTimes(2);
    const messages = recordedMessages();
    expect(messages[1].title).toContain("T2");
  });

  it("AC-011 通知标题、摘要与明细随激活语言在中英文之间切换", async () => {
    const scenario = () => {
      const before = makeConfig([
        makeProvider({
          id: "p1",
          name: "Zen Upstream",
          template_id: "opencode-zen",
          mappings: [],
        }),
      ]);
      const after = makeConfig([
        makeProvider({
          id: "p1",
          name: "Zen Upstream",
          template_id: "opencode-zen",
          mappings: [makeMapping({ local_model: "m", upstream_model: "m" })],
        }),
      ]);
      installInvoke({
        interval: 10,
        templates: [makeView("opencode-zen", "https://zen.test/models")],
        sync: () => ({}),
        configs: [before, after],
      });
    };

    await setLanguage("en");
    const enTitle = tTitle("opencode-zen");
    const enSummary = [tProviderCount(1), tAddedCount(1)].join("; ");
    const enDetail = tDetailProvider("Zen Upstream", "m");
    scenario();
    const enMount = mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);
    enMount.unmount();

    await setLanguage("zh");
    const zhTitle = tTitle("opencode-zen");
    const zhSummary = [tProviderCount(1), tAddedCount(1)].join("; ");
    const zhDetail = tDetailProvider("Zen Upstream", "m");
    scenario();
    const zhMount = mountAutoRefresh();
    await settle();
    await settle(10 * 60_000);
    zhMount.unmount();

    const messages = recordedMessages();
    expect(messages).toHaveLength(2);
    const [enMessage, zhMessage] = messages;
    expect(enMessage.title).toBe(enTitle);
    expect(enMessage.summary).toBe(enSummary);
    expect(enMessage.detail).toBe(enDetail);
    expect(zhMessage.title).toBe(zhTitle);
    expect(zhMessage.summary).toBe(zhSummary);
    expect(zhMessage.detail).toBe(zhDetail);
    expect(enMessage.title).not.toBe(zhMessage.title);
    expect(enMessage.summary).not.toBe(zhMessage.summary);
    expect(enMessage.detail).not.toBe(zhMessage.detail);
  });
});
