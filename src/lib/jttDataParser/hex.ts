import type { HexDirection } from "./types";
import {
  splitSourceLines,
  stripInlineAsciiWhitespace,
  trimAsciiWhitespace,
} from "./lexing";

export type HexConversionResult = { output?: string; error?: string };

const HEX_DIGIT_PATTERN = /^[0-9A-Fa-f]+$/;

function hexToBytes(compact: string): number[] {
  const bytes: number[] = [];
  for (let index = 0; index < compact.length; index += 2) {
    bytes.push(parseInt(compact.slice(index, index + 2), 16));
  }
  return bytes;
}

export type Utf8DecodeResult = { ok: true; text: string } | { ok: false };

export function decodeUtf8Bytes(bytes: number[]): Utf8DecodeResult {
  const codePoints: number[] = [];
  let offset = 0;
  while (offset < bytes.length) {
    const first = bytes[offset];
    let codePoint: number;
    let extra: number;
    let minimum: number;
    if (first < 0x80) {
      codePoint = first;
      extra = 0;
      minimum = 0x00;
    } else if (first >= 0xc2 && first <= 0xdf) {
      codePoint = first & 0x1f;
      extra = 1;
      minimum = 0x80;
    } else if (first >= 0xe0 && first <= 0xef) {
      codePoint = first & 0x0f;
      extra = 2;
      minimum = 0x800;
    } else if (first >= 0xf0 && first <= 0xf4) {
      codePoint = first & 0x07;
      extra = 3;
      minimum = 0x10000;
    } else {
      return { ok: false };
    }
    if (offset + extra >= bytes.length) return { ok: false };
    for (let step = 1; step <= extra; step += 1) {
      const continuation = bytes[offset + step];
      if ((continuation & 0xc0) !== 0x80) return { ok: false };
      codePoint = (codePoint << 6) | (continuation & 0x3f);
    }
    if (codePoint < minimum) return { ok: false };
    if (codePoint >= 0xd800 && codePoint <= 0xdfff) return { ok: false };
    if (codePoint > 0x10ffff) return { ok: false };
    codePoints.push(codePoint);
    offset += extra + 1;
  }
  return { ok: true, text: String.fromCodePoint(...codePoints) };
}

export function hasIsolatedSurrogate(text: string): boolean {
  for (let index = 0; index < text.length; index += 1) {
    const code = text.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = text.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return true;
      index += 1;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      return true;
    }
  }
  return false;
}

export function utf8BytesOf(text: string): number[] {
  const bytes: number[] = [];
  for (let index = 0; index < text.length; index += 1) {
    const code = text.codePointAt(index) as number;
    if (code > 0xffff) index += 1;
    if (code <= 0x7f) {
      bytes.push(code);
    } else if (code <= 0x7ff) {
      bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
    } else if (code <= 0xffff) {
      bytes.push(
        0xe0 | (code >> 12),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
    } else {
      bytes.push(
        0xf0 | (code >> 18),
        0x80 | ((code >> 12) & 0x3f),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
    }
  }
  return bytes;
}

export function convertHexLines(
  input: string,
  direction: HexDirection,
): HexConversionResult {
  if (direction === "hex-to-utf8") {
    return convertHexToUtf8(input);
  }
  return convertUtf8ToHex(input);
}

function convertHexToUtf8(input: string): HexConversionResult {
  const outputLines: string[] = [];
  for (const line of splitSourceLines(input)) {
    const compact = stripInlineAsciiWhitespace(line.text);
    if (compact === "") {
      outputLines.push("");
      continue;
    }
    if (!HEX_DIGIT_PATTERN.test(compact)) {
      return { error: `第 ${line.lineNumber} 行包含非十六进制字符` };
    }
    if (compact.length % 2 !== 0) {
      return { error: `第 ${line.lineNumber} 行的十六进制位数为奇数` };
    }
    const decoded = decodeUtf8Bytes(hexToBytes(compact));
    if (!decoded.ok) {
      return { error: `第 ${line.lineNumber} 行包含无效的 UTF-8 编码` };
    }
    outputLines.push(decoded.text);
  }
  return { output: outputLines.join("\n") };
}

function convertUtf8ToHex(input: string): HexConversionResult {
  const outputLines: string[] = [];
  for (const line of splitSourceLines(input)) {
    if (trimAsciiWhitespace(line.text) === "") {
      outputLines.push("");
      continue;
    }
    if (hasIsolatedSurrogate(line.text)) {
      return { error: `第 ${line.lineNumber} 行包含孤立的 UTF-16 代理项` };
    }
    const bytes = utf8BytesOf(line.text);
    outputLines.push(
      bytes.map((byte) => byte.toString(16).toUpperCase().padStart(2, "0")).join(" "),
    );
  }
  return { output: outputLines.join("\n") };
}