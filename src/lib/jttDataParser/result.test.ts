import { describe, expect, it } from "vitest";
import { serializeRecords } from "./result";

describe("serializeRecords", () => {
  it("serializes a success record tree as an indented plain-text tree with a status line", () => {
    const text = serializeRecords([
      {
        kind: "success",
        tree: [
          { label: "消息 ID", value: "0x0200" },
          {
            label: "帧结构",
            children: [
              { label: "起始标志", value: "0x7E" },
              { label: "校验和", value: "0x76" },
            ],
          },
        ],
      },
    ]);

    expect(text).toBe(
      [
        "状态: 成功",
        "消息 ID: 0x0200",
        "帧结构:",
        "  起始标志: 0x7E",
        "  校验和: 0x76",
      ].join("\n"),
    );
  });

  it("keeps batch records separated with their one-based line number and error details", () => {
    const text = serializeRecords([
      { kind: "success", line: 1, tree: [{ label: "消息 ID", value: "0x0801" }] },
      { kind: "error", line: 3, error: "校验和不匹配", tree: [] },
    ]);

    expect(text).toBe(
      [
        "第 1 行",
        "状态: 成功",
        "消息 ID: 0x0801",
        "第 3 行",
        "状态: 解析失败",
        "说明: 校验和不匹配",
      ].join("\n"),
    );
  });

  it("serializes an unsupported-body record without inventing body fields", () => {
    const text = serializeRecords([
      {
        kind: "unsupported",
        tree: [
          { label: "消息 ID", value: "0x0900" },
          { label: "原始数据体 (Hex)", value: "AABB" },
        ],
      },
    ]);

    expect(text).toBe(
      ["状态: 暂不支持该协议体", "消息 ID: 0x0900", "原始数据体 (Hex): AABB"].join("\n"),
    );
  });
});