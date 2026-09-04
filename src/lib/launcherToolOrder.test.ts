import { beforeEach, describe, expect, it } from "vitest";
import {
  LAUNCHER_INTERNAL_TOOLS_ORDER_KEY,
  MORE_TOOLS_ORDER_KEY,
  applySavedOrder,
  moveItemInList,
  readSavedOrder,
  writeSavedOrder,
} from "@/lib/launcherToolOrder";

describe("launcherToolOrder", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  describe("moveItemInList", () => {
    it("将元素从起始索引移动到目标索引", () => {
      expect(moveItemInList(["a", "b", "c", "d"], 0, 2)).toEqual([
        "b",
        "c",
        "a",
        "d",
      ]);
      expect(moveItemInList(["a", "b", "c", "d"], 3, 0)).toEqual([
        "d",
        "a",
        "b",
        "c",
      ]);
    });

    it("索引相同或越界时返回保持原顺序的新数组", () => {
      const items = ["a", "b", "c"];
      expect(moveItemInList(items, 1, 1)).toEqual(["a", "b", "c"]);
      expect(moveItemInList(items, -1, 1)).toEqual(["a", "b", "c"]);
      expect(moveItemInList(items, 0, 3)).toEqual(["a", "b", "c"]);
    });

    it("不修改原数组", () => {
      const items = ["a", "b", "c"];
      moveItemInList(items, 0, 2);
      expect(items).toEqual(["a", "b", "c"]);
    });
  });

  describe("applySavedOrder", () => {
    it("按保存顺序排列已知 id，其余保持原相对顺序", () => {
      const items = [{ id: "a" }, { id: "b" }, { id: "c" }, { id: "d" }];
      expect(applySavedOrder(items, ["d", "a"])).toEqual([
        { id: "d" },
        { id: "a" },
        { id: "b" },
        { id: "c" },
      ]);
    });

    it("忽略保存顺序中不存在于列表的 id", () => {
      const items = [{ id: "a" }, { id: "b" }];
      expect(applySavedOrder(items, ["z", "b", "y"])).toEqual([
        { id: "b" },
        { id: "a" },
      ]);
    });

    it("无保存顺序时保持原顺序", () => {
      const items = [{ id: "a" }, { id: "b" }];
      expect(applySavedOrder(items, [])).toEqual([{ id: "a" }, { id: "b" }]);
    });
  });

  describe("readSavedOrder / writeSavedOrder", () => {
    it("写入后可读回相同顺序", () => {
      writeSavedOrder(MORE_TOOLS_ORDER_KEY, ["ssh", "cloud", "bookmarks"]);
      expect(readSavedOrder(MORE_TOOLS_ORDER_KEY)).toEqual([
        "ssh",
        "cloud",
        "bookmarks",
      ]);
    });

    it("无存储时返回空数组", () => {
      expect(readSavedOrder(MORE_TOOLS_ORDER_KEY)).toEqual([]);
    });

    it("存储内容损坏时返回空数组", () => {
      localStorage.setItem(MORE_TOOLS_ORDER_KEY, "not-json");
      expect(readSavedOrder(MORE_TOOLS_ORDER_KEY)).toEqual([]);
    });

    it("过滤非字符串项", () => {
      localStorage.setItem(
        MORE_TOOLS_ORDER_KEY,
        JSON.stringify(["a", 3, null]),
      );
      expect(readSavedOrder(MORE_TOOLS_ORDER_KEY)).toEqual(["a"]);
    });

    it("两个网格使用不同存储键", () => {
      expect(LAUNCHER_INTERNAL_TOOLS_ORDER_KEY).not.toBe(MORE_TOOLS_ORDER_KEY);
    });
  });
});