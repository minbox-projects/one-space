import { describe, expect, it } from "vitest";
import {
  convertHexLines,
  decodeUtf8Bytes,
  hasIsolatedSurrogate,
} from "./hex";

describe("hex conversion", () => {
  it("decodes LF-separated hex lines in order while preserving blank output lines", () => {
    expect(convertHexLines("48656C6C6F\n\nE4BDA0E5A5BD", "hex-to-utf8")).toEqual({
      output: "Hello\n\n你好",
    });
  });

  it("ignores ASCII whitespace inside nonblank hex lines", () => {
    expect(convertHexLines("48 65\t6C 6C 6F", "hex-to-utf8")).toEqual({
      output: "Hello",
    });
  });

  it("rejects a non-hex line with its line number and keeps nothing else", () => {
    expect(convertHexLines("4865\n48 6Z", "hex-to-utf8")).toEqual({
      error: "第 2 行包含非十六进制字符",
    });
  });

  it("rejects an odd-length hex line with its line number", () => {
    expect(convertHexLines("4 8 6", "hex-to-utf8")).toEqual({
      error: "第 1 行的十六进制位数为奇数",
    });
  });

  it("rejects a line that is not valid UTF-8 with its line number", () => {
    expect(convertHexLines("48\nC328", "hex-to-utf8")).toEqual({
      error: "第 2 行包含无效的 UTF-8 编码",
    });
  });

  it("rejects a UTF-8 encoded isolated surrogate in the hex-to-utf8 direction", () => {
    expect(convertHexLines("EDA080", "hex-to-utf8")).toEqual({
      error: "第 1 行包含无效的 UTF-8 编码",
    });
  });

  it("encodes each text line as uppercase byte pairs separated by one ASCII space", () => {
    expect(convertHexLines("Hello\n\n你好", "utf8-to-hex")).toEqual({
      output: "48 65 6C 6C 6F\n\nE4 BD A0 E5 A5 BD",
    });
  });

  it("rejects an isolated high or low UTF-16 surrogate in the utf8-to-hex direction", () => {
    expect(convertHexLines("A\uD800", "utf8-to-hex")).toEqual({
      error: "第 1 行包含孤立的 UTF-16 代理项",
    });
    expect(convertHexLines("\uDC00", "utf8-to-hex")).toEqual({
      error: "第 1 行包含孤立的 UTF-16 代理项",
    });
  });

  it("accepts a valid surrogate pair in the utf8-to-hex direction", () => {
    const pair = "\uD83D\uDE00";
    expect(hasIsolatedSurrogate(pair)).toBe(false);
    expect(convertHexLines(pair, "utf8-to-hex").output).toBe("F0 9F 98 80");
  });

  it("turns an empty input into empty output without an error", () => {
    expect(convertHexLines("", "hex-to-utf8")).toEqual({ output: "" });
    expect(convertHexLines("", "utf8-to-hex")).toEqual({ output: "" });
  });
});

describe("decodeUtf8Bytes", () => {
  it("decodes a valid multi-byte UTF-8 sequence", () => {
    expect(decodeUtf8Bytes([0xe4, 0xbd, 0xa0])).toEqual({ ok: true, text: "你" });
  });

  it("decodes a valid two-byte sequence", () => {
    expect(decodeUtf8Bytes([0xc2, 0xa9])).toEqual({ ok: true, text: "©" });
  });

  it("rejects an overlong encoding", () => {
    expect(decodeUtf8Bytes([0xc0, 0xaf])).toEqual({ ok: false });
  });

  it("rejects a UTF-8 encoded surrogate", () => {
    expect(decodeUtf8Bytes([0xed, 0xa0, 0x80])).toEqual({ ok: false });
  });

  it("rejects a truncated multi-byte sequence", () => {
    expect(decodeUtf8Bytes([0xe4, 0xbd])).toEqual({ ok: false });
  });

  it("rejects a missing continuation byte", () => {
    expect(decodeUtf8Bytes([0xc3, 0x28])).toEqual({ ok: false });
  });
});