import { act, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";
import {
  useTemplateAutoRefresh,
  useTemplateAutoRefreshFailures,
} from "./useTemplateAutoRefresh";

// ---------------------------------------------------------------------------
// Frozen Step 2 contract (20261009-core-workflows-cleanup-and-optimization,
// REQ-002 / AC-002). The process (Rust) owns template scheduling; the frontend
// hook is only a read/subscription adapter:
//
//   * on mount it invokes `ai_gateway_template_auto_refresh_status` once and
//     maps `{ failures: [{ template_id, reason }] }` to a template->reason map,
//   * it subscribes to `ai-gateway-template-auto-refresh-updated`, whose payload
//     carries the same snapshot shape, and replaces the map (so reasons clear),
//   * it never schedules refresh timers, never lists templates and never calls
//     `ai_gateway_sync_provider_template`; the backend scheduler does.
//
// The old timer/in-flight/batch-message cases were removed because they expressed
// frontend-owned scheduling that this contract retires. Assertions observe only
// the external IPC boundary: the mocked `@tauri-apps/api/core` invoke command
// stream and the mocked `@tauri-apps/api/event` listen subscription.
// ---------------------------------------------------------------------------

const STATUS_COMMAND = "ai_gateway_template_auto_refresh_status";
const UPDATED_EVENT = "ai-gateway-template-auto-refresh-updated";
const AUTO_REFRESH_GET_COMMAND = "ai_gateway_template_auto_refresh_get";
const AUTO_REFRESH_SAVE_COMMAND = "ai_gateway_template_auto_refresh_save";
const PROVIDER_TEMPLATES_COMMAND = "ai_gateway_provider_templates";
const SYNC_PROVIDER_TEMPLATE_COMMAND = "ai_gateway_sync_provider_template";
const GET_CONFIG_COMMAND = "ai_gateway_get_config";

/** Persisted default interval; the adapter must ignore it entirely. */
const DEFAULT_INTERVAL_MINUTES = 60;

interface FailureSnapshotEntry {
  template_id: string;
  reason: string;
}

interface TemplateViewFixture {
  template: { id: string; models_url: string | null };
  synced_at: number | null;
  source: string;
  from_snapshot: boolean;
}

function makeView(id: string, modelsUrl = `https://${id}.test/models`): TemplateViewFixture {
  return {
    template: { id, models_url: modelsUrl },
    synced_at: null,
    source: "",
    from_snapshot: false,
  };
}

/**
 * Install the command responses the adapter (and, for the current RED, the old
 * frontend scheduler) can call. `ai_gateway_get_config` always returns a valid
 * empty config so the legacy scheduler cannot throw while the tests run.
 */
function installInvoke(options: {
  statusFailures?: FailureSnapshotEntry[];
  interval?: unknown;
  templates?: TemplateViewFixture[];
}): void {
  invokeMock.mockImplementation(async (command: string) => {
    switch (command) {
      case STATUS_COMMAND:
        return { failures: options.statusFailures ?? [] };
      case AUTO_REFRESH_GET_COMMAND:
        return options.interval;
      case PROVIDER_TEMPLATES_COMMAND:
        return options.templates ?? [];
      case GET_CONFIG_COMMAND:
        return {
          enabled: true,
          port: 17688,
          providers: [],
          keys: [],
          default_key_id: null,
          terminal_syncs: [],
        };
      default:
        return undefined;
    }
  });
}

/** Flush promise/timer work while fake timers are installed. */
async function settle(ms = 0): Promise<void> {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

/** Mount only the public hooks; no ToastProvider wrapper is required. */
function mountAutoRefresh() {
  return renderHook(() => {
    useTemplateAutoRefresh();
    return useTemplateAutoRefreshFailures();
  });
}

function invokeCommands(): string[] {
  return invokeMock.mock.calls.map(([command]) => String(command));
}

function statusReads(): number {
  return invokeMock.mock.calls.filter(([command]) => command === STATUS_COMMAND).length;
}

function syncIds(): string[] {
  return invokeMock.mock.calls
    .filter(([command]) => command === SYNC_PROVIDER_TEMPLATE_COMMAND)
    .map(([, args]) => String((args as Record<string, unknown> | undefined)?.templateId));
}

/** Latest subscription handler registered for the backend snapshot event. */
function updatedEventHandler():
  | ((event: { payload?: unknown }) => unknown)
  | undefined {
  return listenMock.mock.calls.find(([event]) => event === UPDATED_EVENT)?.[1];
}

describe("useTemplateAutoRefresh 后端失败快照读/订阅适配器", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    resetTauriMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("mounts reading the backend failure snapshot once and starts no schedule", async () => {
    installInvoke({
      statusFailures: [{ template_id: "t1", reason: "boom" }],
      interval: DEFAULT_INTERVAL_MINUTES,
      templates: [makeView("t1"), makeView("t2")],
    });

    const { result } = mountAutoRefresh();
    await settle();

    // 挂载时读取一次后端状态，并把快照映射为 模板->原因。
    expect(result.current).toEqual({ t1: "boom" });
    expect(statusReads()).toBe(1);

    // 读/订阅适配器绝不触发批次：不列模板、不同步，也不落任何写命令。
    expect(invokeCommands()).not.toContain(PROVIDER_TEMPLATES_COMMAND);
    expect(syncIds()).toEqual([]);
    expect(invokeCommands()).not.toContain(AUTO_REFRESH_SAVE_COMMAND);
  });

  it("never schedules a refresh when fake timers advance past two default intervals", async () => {
    installInvoke({
      statusFailures: [],
      interval: DEFAULT_INTERVAL_MINUTES,
      templates: [makeView("t1")],
    });

    mountAutoRefresh();
    await settle();

    await settle(DEFAULT_INTERVAL_MINUTES * 60_000);
    await settle(DEFAULT_INTERVAL_MINUTES * 60_000);

    // 无论经过多少个间隔，前端都不再抓取/同步任何模板。
    expect(syncIds()).toEqual([]);
    expect(invokeCommands()).not.toContain(PROVIDER_TEMPLATES_COMMAND);
    expect(statusReads()).toBe(1);
  });

  it("subscribes to the backend snapshot event and replaces the failure map", async () => {
    installInvoke({
      statusFailures: [{ template_id: "t1", reason: "boom" }],
      interval: DEFAULT_INTERVAL_MINUTES,
      templates: [],
    });

    const { result } = mountAutoRefresh();
    await settle();

    const handler = updatedEventHandler();
    expect(handler).toBeTypeOf("function");
    expect(result.current).toEqual({ t1: "boom" });

    // 事件携带清空后的快照：失败原因随之后端快照被清除。
    act(() => {
      handler?.({ payload: { failures: [] } });
    });
    expect(result.current).toEqual({});
  });

  it("exposes backend failures without any toast side effect", async () => {
    installInvoke({
      statusFailures: [{ template_id: "t1", reason: "boom" }],
      interval: DEFAULT_INTERVAL_MINUTES,
      templates: [],
    });

    const { result } = mountAutoRefresh();
    await settle();

    expect(result.current).toEqual({ t1: "boom" });
    // 失败只在卡片内联展示，hook 绝不弹 toast。
    expect(screen.queryByText(/boom/)).not.toBeInTheDocument();
  });
});
