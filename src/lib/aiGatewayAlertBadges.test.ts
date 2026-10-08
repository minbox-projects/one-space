import { beforeEach, describe, expect, it } from "vitest";
import {
  AI_GATEWAY_ALERT_BADGES_STORAGE_KEY,
  autoDisabledAlertInstanceKey,
  dismissAlertInstanceKeys,
  readDismissedAlertInstanceKeys,
  reconcileDismissedAlertInstanceKeys,
  retiredMappingAlertInstanceKey,
} from "@/lib/aiGatewayAlertBadges";

// A minimal in-memory Storage so every test owns its persisted bytes instead of
// sharing jsdom's window.localStorage.
function createMemoryStorage(initial: Record<string, string> = {}): Storage {
  const map = new Map<string, string>(Object.entries(initial));
  return {
    get length() {
      return map.size;
    },
    clear() {
      map.clear();
    },
    getItem(key: string) {
      return map.has(key) ? (map.get(key) as string) : null;
    },
    key(index: number) {
      return Array.from(map.keys())[index] ?? null;
    },
    removeItem(key: string) {
      map.delete(key);
    },
    setItem(key: string, value: string) {
      map.set(key, value);
    },
  } as Storage;
}

function createThrowingStorage(): Storage {
  const fail = () => {
    throw new Error("storage unavailable");
  };
  return {
    get length() {
      return fail();
    },
    clear: fail,
    getItem: fail,
    key: fail,
    removeItem: fail,
    setItem: fail,
  } as unknown as Storage;
}

beforeEach(() => {
  window.localStorage.clear();
});

describe("告警实例键格式", () => {
  it("自动禁用实例键为 auto:provider:local:upstream", () => {
    expect(autoDisabledAlertInstanceKey("p1", "local-a", "remote-a")).toBe(
      "auto:p1:local-a:remote-a",
    );
  });

  it("自动禁用实例键裁剪本地模型并在为空时回退到上游模型", () => {
    expect(autoDisabledAlertInstanceKey("p1", "  local-a  ", "remote-a")).toBe(
      "auto:p1:local-a:remote-a",
    );
    expect(autoDisabledAlertInstanceKey("p1", "   ", "remote-a")).toBe(
      "auto:p1:remote-a:remote-a",
    );
    expect(autoDisabledAlertInstanceKey("p1", "", "remote-a")).toBe(
      "auto:p1:remote-a:remote-a",
    );
  });

  it("退休映射实例键为 retired:provider:template:upstream", () => {
    expect(retiredMappingAlertInstanceKey("p1", "tpl-1", "gone-a")).toBe(
      "retired:p1:tpl-1:gone-a",
    );
  });

  it("两类实例键不会互相碰撞", () => {
    expect(autoDisabledAlertInstanceKey("p1", "tpl-1", "gone-a")).not.toBe(
      retiredMappingAlertInstanceKey("p1", "tpl-1", "gone-a"),
    );
  });

  it("存储键是非空字符串", () => {
    expect(typeof AI_GATEWAY_ALERT_BADGES_STORAGE_KEY).toBe("string");
    expect(AI_GATEWAY_ALERT_BADGES_STORAGE_KEY.length).toBeGreaterThan(0);
  });
});

describe("关闭实例的持久化读写", () => {
  it("关闭后按存储键写入 JSON 字符串数组并可读回", () => {
    const storage = createMemoryStorage();

    const merged = dismissAlertInstanceKeys(["auto:p1:l1:u1"], storage);
    expect(merged.has("auto:p1:l1:u1")).toBe(true);

    const raw = storage.getItem(AI_GATEWAY_ALERT_BADGES_STORAGE_KEY);
    expect(raw).not.toBeNull();
    const parsed = JSON.parse(raw as string);
    expect(Array.isArray(parsed)).toBe(true);
    expect(parsed).toEqual(["auto:p1:l1:u1"]);

    expect(readDismissedAlertInstanceKeys(storage)).toEqual(
      new Set(["auto:p1:l1:u1"]),
    );
  });

  it("合并是追加式的，且接受任意可迭代输入", () => {
    const storage = createMemoryStorage();

    dismissAlertInstanceKeys(["auto:p1:l1:u1"], storage);
    const merged = dismissAlertInstanceKeys(
      new Set(["retired:p1:t1:u2"]),
      storage,
    );

    expect(merged).toEqual(
      new Set(["auto:p1:l1:u1", "retired:p1:t1:u2"]),
    );
    expect(readDismissedAlertInstanceKeys(storage)).toEqual(
      new Set(["auto:p1:l1:u1", "retired:p1:t1:u2"]),
    );
  });

  it("省略 storage 时使用真实的 window.localStorage", () => {
    dismissAlertInstanceKeys(["auto:p-default:l:u"]);
    expect(readDismissedAlertInstanceKeys().has("auto:p-default:l:u")).toBe(true);
    expect(
      window.localStorage.getItem(AI_GATEWAY_ALERT_BADGES_STORAGE_KEY),
    ).toContain("auto:p-default:l:u");
  });
});

describe("损坏或不可用存储的安全降级", () => {
  it("非 JSON 文本返回空集合且关闭仍返回会话集合", () => {
    const storage = createMemoryStorage({
      [AI_GATEWAY_ALERT_BADGES_STORAGE_KEY]: "{not json",
    });

    expect(readDismissedAlertInstanceKeys(storage)).toEqual(new Set());

    const merged = dismissAlertInstanceKeys(["auto:p-corrupt:l:u"], storage);
    expect(merged.has("auto:p-corrupt:l:u")).toBe(true);
  });

  it("JSON 非数组返回空集合", () => {
    const storage = createMemoryStorage({
      [AI_GATEWAY_ALERT_BADGES_STORAGE_KEY]: '{"a":1}',
    });

    expect(readDismissedAlertInstanceKeys(storage)).toEqual(new Set());
  });

  it("读写抛错的 Storage 不抛出，读取返回空集合、关闭返回会话集合", () => {
    const storage = createThrowingStorage();

    expect(() => readDismissedAlertInstanceKeys(storage)).not.toThrow();
    expect(readDismissedAlertInstanceKeys(storage)).toEqual(new Set());

    let merged: Set<string> | undefined;
    expect(() => {
      merged = dismissAlertInstanceKeys(["auto:p-throw:l:u"], storage);
    }).not.toThrow();
    expect(merged?.has("auto:p-throw:l:u")).toBe(true);
  });

  it("显式 null 表示存储不可用且不抛出", () => {
    expect(() => readDismissedAlertInstanceKeys(null)).not.toThrow();
    expect(readDismissedAlertInstanceKeys(null)).toEqual(new Set());

    expect(() => reconcileDismissedAlertInstanceKeys(["auto:p:l:u"], null)).not.toThrow();
    expect(reconcileDismissedAlertInstanceKeys(["auto:p:l:u"], null)).toEqual(
      new Set(),
    );

    let merged: Set<string> | undefined;
    expect(() => {
      merged = dismissAlertInstanceKeys(["auto:p-null:l:u"], null);
    }).not.toThrow();
    expect(merged?.has("auto:p-null:l:u")).toBe(true);
  });
});

describe("按当前问题集修剪关闭记录", () => {
  it("仅保留当前键并持久化修剪后的集合", () => {
    const storage = createMemoryStorage();
    dismissAlertInstanceKeys(
      ["auto:p:l1:u1", "auto:p:l2:u2", "retired:p:t1:u3"],
      storage,
    );

    const kept = reconcileDismissedAlertInstanceKeys(["auto:p:l1:u1"], storage);

    expect(kept).toEqual(new Set(["auto:p:l1:u1"]));
    expect(readDismissedAlertInstanceKeys(storage)).toEqual(
      new Set(["auto:p:l1:u1"]),
    );
  });

  it("被修剪的实例再次出现时不再算已关闭，需重新关闭后才记住", () => {
    const storage = createMemoryStorage();
    dismissAlertInstanceKeys(["auto:p:l2:u2"], storage);
    // 问题已解决：当前问题集不含 l2，关闭记录被清除。
    expect(reconcileDismissedAlertInstanceKeys([], storage)).toEqual(new Set());

    // 复发：同一实例键不再是已关闭状态。
    const recurrence = reconcileDismissedAlertInstanceKeys(
      ["auto:p:l2:u2"],
      storage,
    );
    expect(recurrence.has("auto:p:l2:u2")).toBe(false);
    expect(readDismissedAlertInstanceKeys(storage).size).toBe(0);

    // 再次关闭后才重新记住。
    const redismissed = dismissAlertInstanceKeys(["auto:p:l2:u2"], storage);
    expect(redismissed.has("auto:p:l2:u2")).toBe(true);
    expect(readDismissedAlertInstanceKeys(storage).has("auto:p:l2:u2")).toBe(
      true,
    );
  });
});
