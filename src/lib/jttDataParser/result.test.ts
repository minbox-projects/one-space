import { describe, expect, it } from "vitest";
import { serializeRecords } from "./result";

describe("serializeRecords", () => {
  it("serializes a success record tree as an indented plain-text tree", () => {
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
      ["消息 ID: 0x0200", "帧结构:", "  起始标志: 0x7E", "  校验和: 0x76"].join("\n"),
    );
  });

  it("serializes a success json record as the pretty-printed JSON only", () => {
    const text = serializeRecords([
      {
        kind: "success",
        line: 1,
        tree: [],
        json: { "[7E]开始": 126, "[0200]消息Id": 512 },
      },
    ]);

    expect(text).toBe('{\n  "[7E]开始": 126,\n  "[0200]消息Id": 512\n}');
  });

  it("skips error records and keeps only successful results, separated by a blank line", () => {
    const text = serializeRecords([
      { kind: "success", line: 1, tree: [{ label: "消息 ID", value: "0x0801" }] },
      { kind: "error", line: 3, error: "校验和不匹配", tree: [] },
      { kind: "success", line: 4, tree: [{ label: "消息 ID", value: "0x0200" }] },
    ]);

    expect(text).toBe("消息 ID: 0x0801\n\n消息 ID: 0x0200");
  });

  it("omits unsupported-body records from the copied result", () => {
    const text = serializeRecords([
      {
        kind: "unsupported",
        tree: [
          { label: "消息 ID", value: "0x0900" },
          { label: "原始数据体 (Hex)", value: "AABB" },
        ],
      },
    ]);

    expect(text).toBe("");
  });
});