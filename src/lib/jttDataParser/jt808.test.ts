import { describe, expect, it } from "vitest";
import { analyzeJt808, buildJt808LocationNodes, jt808Modes } from "./jt808";
import {
  JT808_BAD_CHECKSUM,
  JT808_BAD_ESCAPE,
  JT808_1078_EXT_0X9101,
  JT808_CONFLICT_FRAGMENT_1,
  JT808_CONFLICT_FRAGMENT_2,
  JT808_DUP_FRAGMENT_1,
  JT808_DUP_FRAGMENT_2,
  JT808_F1_2013_0801_ESCAPED,
  JT808_F2_2011_0801,
  JT808_F3_FRAGMENT_1,
  JT808_F3_FRAGMENT_2,
  JT808_F3_FRAGMENT_3,
  JT808_GUANGDONG_0A01,
  JT808_JIANGSU_0B01,
  JT808_MISS_FRAGMENT_1,
  JT808_MISS_FRAGMENT_3,
  JT808_NO_START,
  JT808_POSITION_0200,
  JT808_POSITION_0704_TWO_FRAMES,
  JT808_TRUNCATED,
} from "./fixtures";
import { parseJt808Wire } from "./frame";
import { findNode, findNodeValue, nodeLabels } from "./testUtils";

function readUint32BE(bytes: number[], offset: number): number {
  return (
    (((bytes[offset] << 24) |
      (bytes[offset + 1] << 16) |
      (bytes[offset + 2] << 8) |
      bytes[offset + 3]) >>>
      0)
  );
}

function hexToBytes(hex: string): number[] {
  const bytes: number[] = [];
  for (let index = 0; index < hex.length; index += 2) {
    bytes.push(parseInt(hex.slice(index, index + 2), 16));
  }
  return bytes;
}

function bytesToHex(bytes: number[]): string {
  return bytes.map((byte) => byte.toString(16).toUpperCase().padStart(2, "0")).join("");
}

function unescapeJt808(bytes: number[]): number[] {
  const output: number[] = [];
  for (let index = 0; index < bytes.length; index += 1) {
    if (bytes[index] === 0x7d && bytes[index + 1] === 0x01) {
      output.push(0x7d);
      index += 1;
    } else if (bytes[index] === 0x7d && bytes[index + 1] === 0x02) {
      output.push(0x7e);
      index += 1;
    } else {
      output.push(bytes[index]);
    }
  }
  return output;
}

function fixtureInner(fixture: string): number[] {
  const bytes = hexToBytes(fixture);
  return unescapeJt808(bytes.slice(1, -1));
}

function fixtureBody(fixture: string): number[] {
  const inner = fixtureInner(fixture);
  const bodyLength = ((inner[2] << 8) | inner[3]) & 0x3ff;
  return inner.slice(12, 12 + bodyLength);
}

// 0x0801 多媒体上传: 8-byte media header, then the 28-byte position block.
function fixture0801PositionBlock(fixture: string): number[] {
  return fixtureBody(fixture).slice(8, 36);
}

function fixtureChecksum(fixture: string): number {
  const inner = fixtureInner(fixture);
  const bodyLength = ((inner[2] << 8) | inner[3]) & 0x3ff;
  return inner
    .slice(0, 12 + bodyLength)
    .reduce((acc, byte) => acc ^ byte, 0);
}

describe("analyzeJt808", () => {
  it("parses an escaped 2013 0x0801 frame and decodes the escaped bytes", () => {
    const [record] = analyzeJt808(JT808_F1_2013_0801_ESCAPED, "automatic");
    const position = fixture0801PositionBlock(JT808_F1_2013_0801_ESCAPED);
    const checksum = fixtureChecksum(JT808_F1_2013_0801_ESCAPED);

    expect(record.kind).toBe("success");
    expect(record.line).toBe(1);
    expect(findNodeValue(record.tree, "消息 ID")).toBe("0x0801");
    expect(findNodeValue(record.tree, "消息体长度")).toBe("42");
    expect(findNodeValue(record.tree, "分包")).toBe("否");
    expect(findNodeValue(record.tree, "版本")).toBe("2013");
    expect(findNodeValue(record.tree, "终端手机号")).toBe("013123456789");
    expect(findNodeValue(record.tree, "消息流水号")).toBe("1024");
    expect(findNodeValue(record.tree, "校验和")).toBe(
      `0x${checksum.toString(16).toUpperCase().padStart(2, "0")}`,
    );
    expect(findNodeValue(record.tree, "多媒体类型")).toBe("0x02 (音频)");
    expect(findNodeValue(record.tree, "多媒体格式编码")).toBe("0x03 (MP3)");
    expect(findNodeValue(record.tree, "多媒体数据 (Hex)")).toBe("01027E7D0304");
    expect(findNodeValue(record.tree, "纬度")).toBe(String(readUint32BE(position, 8)));
    expect(findNodeValue(record.tree, "经度")).toBe(String(readUint32BE(position, 12)));
    expect(findNodeValue(record.tree, "时间")).toBe("2026-09-04 14:30:00");
  });

  it("reads the same frame as 2011 in automatic mode when no version marker is present", () => {
    const [record] = analyzeJt808(JT808_F2_2011_0801, "automatic");
    const f2Body = fixtureBody(JT808_F2_2011_0801);

    expect(record.kind).toBe("success");
    expect(findNodeValue(record.tree, "版本")).toBe("2011");
    expect(findNode(record.tree, "位置信息")).toBeUndefined();
    expect(findNodeValue(record.tree, "多媒体数据 (Hex)")).toBe(
      bytesToHex(f2Body.slice(8)),
    );
  });

  it("interprets the version-sensitive fixture as 2013 in force-2013 mode", () => {
    const [record] = analyzeJt808(JT808_F2_2011_0801, "force-2013");
    const position = fixture0801PositionBlock(JT808_F2_2011_0801);

    expect(record.kind).toBe("success");
    expect(findNodeValue(record.tree, "版本")).toBe("2013");
    expect(findNodeValue(record.tree, "纬度")).toBe(String(readUint32BE(position, 8)));
    expect(findNodeValue(record.tree, "经度")).toBe(String(readUint32BE(position, 12)));
    expect(findNodeValue(record.tree, "多媒体数据 (Hex)")).toBe("AABBCCDD");
  });

  it("reports a structurally valid unlisted body as unsupported with frame fields", () => {
    const [record] = analyzeJt808(JT808_F2_2011_0801, "guangdong-active-safety");

    expect(record.kind).toBe("unsupported");
    expect(findNodeValue(record.tree, "消息 ID")).toBe("0x0801");
    expect(findNodeValue(record.tree, "支持状态")).toBe("不在本模式冻结支持范围内");
  });

  it("parses a Jiangsu active-safety body only in its mode", () => {
    const [jiangsu] = analyzeJt808(JT808_JIANGSU_0B01, "jiangsu-active-safety");
    expect(jiangsu.kind).toBe("success");
    expect(nodeLabels(jiangsu.tree)).toContain("协议体 (0x0B01 江苏主动安全报警)");

    const [automatic] = analyzeJt808(JT808_JIANGSU_0B01, "automatic");
    expect(automatic.kind).toBe("unsupported");
  });

  it("parses a Guangdong active-safety body only in its mode", () => {
    const [guangdong] = analyzeJt808(JT808_GUANGDONG_0A01, "guangdong-active-safety");
    expect(guangdong.kind).toBe("success");
    expect(nodeLabels(guangdong.tree)).toContain("协议体 (0x0A01 广东主动安全报警)");

    const [automatic] = analyzeJt808(JT808_GUANGDONG_0A01, "automatic");
    expect(automatic.kind).toBe("unsupported");
  });

  it("parses a JT1078 linkage body in the JT1078 extension mode", () => {
    const [record] = analyzeJt808(JT808_1078_EXT_0X9101, "jt1078-extension");

    expect(record.kind).toBe("success");
    expect(findNodeValue(record.tree, "服务器地址")).toBe("192.168.1.100");
  });

  it("merges continuous subpackage indexes 1..N within one group", () => {
    const records = analyzeJt808(
      [JT808_F3_FRAGMENT_1, JT808_F3_FRAGMENT_2, JT808_F3_FRAGMENT_3].join("\n"),
      "automatic",
    );

    expect(records).toHaveLength(3);
    expect(records.every((record) => record.kind === "success")).toBe(true);
    for (const record of records) {
      expect(findNodeValue(record.tree, "分组")).toBe("2019|013123456789|0x0801|4132");
      expect(findNodeValue(record.tree, "已收集包序号")).toBe("[1, 2, 3]");
      expect(findNodeValue(record.tree, "组装状态")).toBe("完整合并");
    }
    expect(findNodeValue(records[0].tree, "分包信息")).toBeUndefined();
    expect(findNodeValue(records[0].tree, "总包数")).toBe("3");
    expect(findNodeValue(records[0].tree, "多媒体数据 (Hex)")).toBe("01020304");
    expect(findNodeValue(records[1].tree, "分包数据 (Hex)")).toBe("05067E070809");
  });

  it("keeps a duplicate package index from merging", () => {
    const records = analyzeJt808(
      [JT808_DUP_FRAGMENT_1, JT808_DUP_FRAGMENT_2].join("\n"),
      "automatic",
    );

    for (const record of records) {
      expect(findNodeValue(record.tree, "组装状态")).toBe("包序号重复");
    }
  });

  it("keeps a missing package index from merging", () => {
    const records = analyzeJt808(
      [JT808_MISS_FRAGMENT_1, JT808_MISS_FRAGMENT_3].join("\n"),
      "automatic",
    );

    for (const record of records) {
      expect(findNodeValue(record.tree, "组装状态")).toBe("缺少分包");
    }
  });

  it("keeps a declared-total conflict from merging", () => {
    const records = analyzeJt808(
      [JT808_CONFLICT_FRAGMENT_1, JT808_CONFLICT_FRAGMENT_2].join("\n"),
      "automatic",
    );

    for (const record of records) {
      expect(findNodeValue(record.tree, "组装状态")).toBe("总包数冲突");
    }
  });

  it("produces ordered records for nonblank lines with original line numbers", () => {
    const records = analyzeJt808(
      `${JT808_F1_2013_0801_ESCAPED}\n\n \n${JT808_F1_2013_0801_ESCAPED}\n`,
      "automatic",
    );

    expect(records).toHaveLength(2);
    expect(records.map((record) => record.line)).toEqual([1, 4]);
  });

  it("keeps a line-specific failure record for an invalid nonblank line", () => {
    const records = analyzeJt808(
      `${JT808_BAD_CHECKSUM}\n${JT808_F1_2013_0801_ESCAPED}`,
      "automatic",
    );

    expect(records).toHaveLength(2);
    expect(records[0]).toMatchObject({ kind: "error", line: 1, error: "校验和不匹配" });
    expect(records[1].kind).toBe("success");
  });

  it("parses back-to-back frames on one line as separate ordered records", () => {
    const records = analyzeJt808(JT808_POSITION_0704_TWO_FRAMES, "automatic");

    expect(records).toHaveLength(2);
    expect(records.every((record) => record.kind === "success")).toBe(true);
    expect(records.map((record) => record.line)).toEqual([1, 1]);
    const jsons = records.map((record) => record.json as Record<string, unknown>);
    expect(jsons[0]["[0065]消息流水号"]).toBe(101);
    expect(jsons[1]["[0066]消息流水号"]).toBe(102);
  });

  it.each([
    [JT808_NO_START, "缺少起始标志 0x7E"],
    [JT808_TRUNCATED, "报文长度不足，帧被截断"],
    [JT808_BAD_CHECKSUM, "校验和不匹配"],
    [JT808_BAD_ESCAPE, "转义序列无效"],
  ])("reports a specific framing error for invalid input", (input, error) => {
    const [record] = analyzeJt808(input, "automatic");
    expect(record.kind).toBe("error");
    expect(record.error).toBe(error);
  });

  it("labels the 0x0200 position block with latitude at byte offset 8 and longitude at byte offset 12", () => {
    const parsed = parseJt808Wire(JT808_POSITION_0200);
    if (!parsed.ok) throw new Error(parsed.error);

    const body = parsed.header.body;
    const expectedLatitude = readUint32BE(body, 8);
    const expectedLongitude = readUint32BE(body, 12);

    const nodes = buildJt808LocationNodes(body);

    expect(findNodeValue(nodes, "纬度")).toBe(String(expectedLatitude));
    expect(findNodeValue(nodes, "经度")).toBe(String(expectedLongitude));
  });

  it("exposes only the five public modes and excludes Ruiding and GPS51", () => {
    expect(jt808Modes()).toEqual([
      "automatic",
      "jt1078-extension",
      "jiangsu-active-safety",
      "guangdong-active-safety",
      "force-2013",
    ]);
  });
});