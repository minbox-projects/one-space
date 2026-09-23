import { createElement, type ReactNode } from "react";
import { act, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ToastProvider";
import {
  notifyTemplateAutoRefreshIntervalChanged,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateView,
} from "@/lib/apiGateway";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import {
  clearTemplateAutoRefreshFailures,
  clearTemplateSyncInFlight,
  isTemplateSyncInFlight,
  setTemplateAutoRefreshFailure,
  setTemplateSyncInFlight,
  useTemplateAutoRefresh,
  useTemplateAutoRefreshFailures,
} from "./useTemplateAutoRefresh";

const AUTO_REFRESH_GET_COMMAND = "api_gateway_template_auto_refresh_get";
const PROVIDER_TEMPLATES_COMMAND = "api_gateway_provider_templates";
const SYNC_PROVIDER_TEMPLATE_COMMAND = "api_gateway_sync_provider_template";

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

function installInvoke(options: {
  interval: unknown;
  templates: GatewayProviderTemplateView[];
  sync?: SyncHandler;
}): void {
  let syncIndex = 0;
  invokeMock.mockImplementation(
    async (command: string, args?: Record<string, unknown>) => {
      switch (command) {
        case AUTO_REFRESH_GET_COMMAND:
          return options.interval;
        case PROVIDER_TEMPLATES_COMMAND:
          return options.templates;
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
    clearTemplateSyncInFlight();
    clearTemplateAutoRefreshFailures();
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
});
