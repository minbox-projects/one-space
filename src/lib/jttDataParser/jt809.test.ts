import { describe, expect, it } from "vitest";
import { analyzeJt809, validateJt809Params } from "./jt809";
import {
  JT809_2011_ENCRYPTED_1200,
  JT809_2011_UNENCRYPTED_1200,
  JT809_2019_ENCRYPTED_1200,
  JT809_2019_UNENCRYPTED_0200,
  JT809_2019_UNENCRYPTED_1100,
} from "./fixtures";
import { findNodeValue } from "./testUtils";

const VALID_PARAMS = { m1: "0", ia1: "0", ic1: "0" };

function readUint32BE(bytes: number[], offset: number): number {
  return (
    (((bytes[offset] << 24) |
      (bytes[offset + 1] << 16) |
      (bytes[offset + 2] << 8) |
      bytes[offset + 3]) >>>
      0)
  );
}

function positionBlockBytes(frameHex: string): number[] {
  const bytes: number[] = [];
  for (let index = 0; index < frameHex.length; index += 2) {
    bytes.push(parseInt(frameHex.slice(index, index + 2), 16));
  }
  const bodyLength =
    (((bytes[23] << 24) |
      (bytes[24] << 16) |
      (bytes[25] << 8) |
      bytes[26]) >>>
      0);
  const body = bytes.slice(27, 27 + bodyLength);
  const plateEnd = body.indexOf(0);
  const locationStart = plateEnd + 2;
  return body.slice(locationStart, locationStart + 28);
}

function crc16Ccitt(bytes: number[]): number {
  let crc = 0xffff;
  for (const byte of bytes) {
    crc ^= byte << 8;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc & 0x8000) ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  return crc;
}

function frameCrc(frameHex: string): number {
  const bytes: number[] = [];
  for (let index = 0; index < frameHex.length; index += 2) {
    bytes.push(parseInt(frameHex.slice(index, index + 2), 16));
  }
  const length =
    (((bytes[0] << 24) | (bytes[1] << 16) | (bytes[2] << 8) | bytes[3]) >>> 0);
  return crc16Ccitt(bytes.slice(22, 4 + length - 2));
}

describe("analyzeJt809", () => {
  it("renders a 2019 unencrypted 0x0200 body through the frozen JT808 bridge", () => {
    const record = analyzeJt809(
      JT809_2019_UNENCRYPTED_0200,
      "2019",
      "unencrypted",
      VALID_PARAMS,
    );
    const position = positionBlockBytes(JT809_2019_UNENCRYPTED_0200);
    const crc = frameCrc(JT809_2019_UNENCRYPTED_0200);

    expect(record.kind).toBe("success");
    expect(findNodeValue(record.tree, "报文类型")).toBe("0x0200");
    expect(findNodeValue(record.tree, "报文长度")).toBe("64");
    expect(findNodeValue(record.tree, "报文序列号")).toBe("4660");
    expect(findNodeValue(record.tree, "加密标识")).toBe("0x00 (不加密)");
    expect(findNodeValue(record.tree, "校验码")).toBe(
      `0x${crc.toString(16).toUpperCase().padStart(4, "0")}`,
    );
    expect(findNodeValue(record.tree, "车牌号")).toBe("苏A12345");
    expect(findNodeValue(record.tree, "车辆颜色")).toBe("0x01");
    expect(findNodeValue(record.tree, "纬度")).toBe(String(readUint32BE(position, 8)));
    expect(findNodeValue(record.tree, "经度")).toBe(String(readUint32BE(position, 12)));
    expect(findNodeValue(record.tree, "时间")).toBe("2026-09-04 14:30:00");
  });

  it("labels the bridge position block with latitude at byte offset 8 and longitude at byte offset 12", () => {
    const position = positionBlockBytes(JT809_2019_UNENCRYPTED_0200);
    const expectedLatitude = readUint32BE(position, 8);
    const expectedLongitude = readUint32BE(position, 12);

    const record = analyzeJt809(
      JT809_2019_UNENCRYPTED_0200,
      "2019",
      "unencrypted",
      VALID_PARAMS,
    );

    expect(findNodeValue(record.tree, "纬度")).toBe(String(expectedLatitude));
    expect(findNodeValue(record.tree, "经度")).toBe(String(expectedLongitude));
  });

  it("marks every 2011 body as unsupported while keeping frame fields", () => {
    const record = analyzeJt809(
      JT809_2011_UNENCRYPTED_1200,
      "2011",
      "unencrypted",
      VALID_PARAMS,
    );

    expect(record.kind).toBe("unsupported");
    expect(findNodeValue(record.tree, "报文类型")).toBe("0x1200");
    expect(findNodeValue(record.tree, "原始数据体 (Hex)")).toBe("0102030405");
    expect(findNodeValue(record.tree, "支持状态")).toBe("不在本模式冻结支持范围内");
  });

  it("marks a 2019 non-0x0200 body as unsupported while keeping frame fields", () => {
    const record = analyzeJt809(
      JT809_2019_UNENCRYPTED_1100,
      "2019",
      "unencrypted",
      VALID_PARAMS,
    );

    expect(record.kind).toBe("unsupported");
    expect(findNodeValue(record.tree, "报文类型")).toBe("0x1100");
    expect(findNodeValue(record.tree, "支持状态")).toBe("不在本模式冻结支持范围内");
  });

  it("renders an encrypted body without claiming decryption", () => {
    const record = analyzeJt809(
      JT809_2019_ENCRYPTED_1200,
      "2019",
      "encrypted",
      VALID_PARAMS,
    );

    expect(record.kind).toBe("success");
    expect(findNodeValue(record.tree, "加密标识")).toBe("0x02 (加密)");
    expect(findNodeValue(record.tree, "加密状态")).toBe(
      "数据体已加密，冻结范围内无可审计的公开解密规范",
    );
    expect(findNodeValue(record.tree, "原始数据体 (Hex)")).toBe("ABCD0001");
    expect(findNodeValue(record.tree, "解密后")).toBeUndefined();
  });

  it("applies the same constrained encrypted rendering to 2011", () => {
    const record = analyzeJt809(
      JT809_2011_ENCRYPTED_1200,
      "2011",
      "encrypted",
      VALID_PARAMS,
    );

    expect(record.kind).toBe("success");
    expect(findNodeValue(record.tree, "加密状态")).toBe(
      "数据体已加密，冻结范围内无可审计的公开解密规范",
    );
  });

  it("fails with a single-packet error on an internal newline", () => {
    const record = analyzeJt809(
      `${JT809_2019_UNENCRYPTED_0200}\n${JT809_2019_UNENCRYPTED_1100}`,
      "2019",
      "unencrypted",
      VALID_PARAMS,
    );

    expect(record.kind).toBe("error");
    expect(record.error).toBe("仅支持单条报文，输入不能包含换行");
  });

  it("fails on an empty input", () => {
    const record = analyzeJt809("", "2019", "unencrypted", VALID_PARAMS);
    expect(record.kind).toBe("error");
    expect(record.error).toBe("输入为空");
  });

  it("rejects an encrypted packet whose parameters are outside decimal uint32", () => {
    for (const [field, raw] of [
      ["m1", ""],
      ["ia1", "   "],
      ["ic1", "-1"],
      ["m1", "3.5"],
      ["ia1", "abc"],
      ["ic1", "4294967296"],
    ] as const) {
      const params = { m1: "0", ia1: "0", ic1: "0", [field]: raw };
      const record = analyzeJt809(
        JT809_2019_ENCRYPTED_1200,
        "2019",
        "encrypted",
        params,
      );
      expect(record.kind).toBe("error");
      expect(record.error).toContain(field.toUpperCase());
    }
  });

  it("detects a wrong CRC with a specific error", () => {
    const corrupted = `${JT809_2019_UNENCRYPTED_0200.slice(0, -8)}FEFE7B7E`;
    const record = analyzeJt809(corrupted, "2019", "unencrypted", VALID_PARAMS);
    expect(record.kind).toBe("error");
    expect(record.error).toBe("校验码不匹配");
  });

  it("detects a missing tail with a specific error", () => {
    const record = analyzeJt809(
      JT809_2019_UNENCRYPTED_0200.slice(0, -4),
      "2019",
      "unencrypted",
      VALID_PARAMS,
    );
    expect(record.kind).toBe("error");
    expect(record.error).toBe("缺少结束标志 0x7B 0x7E");
  });
});

describe("validateJt809Params", () => {
  it("accepts decimal integers from 0 through 4294967295", () => {
    expect(validateJt809Params({ m1: "0", ia1: "4294967295", ic1: "123456789" })).toEqual({
      ok: true,
      values: { m1: 0, ia1: 4294967295, ic1: 123456789 },
    });
  });

  it.each([
    ["", "missing"],
    ["   ", "missing"],
    ["-1", "signed"],
    ["+5", "signed"],
    ["1.5", "fractional"],
    ["12a", "nonnumeric"],
    ["4294967296", "out-of-range"],
  ])("rejects raw %j", (raw) => {
    const result = validateJt809Params({ m1: raw, ia1: "0", ic1: "0" });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain("M1");
  });
});