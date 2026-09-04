import type { ResultNode } from "./types";
import { stripInlineAsciiWhitespace } from "./lexing";

const HEX_DIGIT_PATTERN = /^[0-9A-Fa-f]+$/;

export type Jt808ParseError = { ok: false; error: string };

export type Jt808Version = "2011" | "2013" | "2019";

export type ParsedJt808Header = {
  messageId: number;
  bodyProperty: number;
  bodyLength: number;
  encryption: number;
  subpackage: boolean;
  versionBits: number;
  terminal: string;
  serial: number;
  total?: number;
  index?: number;
  body: number[];
  checksum: number;
};

export type ParsedJt808Wire = { ok: true; header: ParsedJt808Header };

function hexToBytes(compact: string): number[] {
  const bytes: number[] = [];
  for (let index = 0; index < compact.length; index += 2) {
    bytes.push(parseInt(compact.slice(index, index + 2), 16));
  }
  return bytes;
}

function unescapeWire(bytes: number[]): number[] | null {
  const output: number[] = [];
  for (let index = 0; index < bytes.length; index += 1) {
    const byte = bytes[index];
    if (byte === 0x7d) {
      const next = bytes[index + 1];
      if (next === 0x01) {
        output.push(0x7d);
        index += 1;
      } else if (next === 0x02) {
        output.push(0x7e);
        index += 1;
      } else {
        return null;
      }
    } else {
      output.push(byte);
    }
  }
  return output;
}

function bcdDigits(bytes: number[]): string {
  let digits = "";
  for (const byte of bytes) {
    digits += ((byte >> 4) & 0x0f).toString();
    digits += (byte & 0x0f).toString();
  }
  return digits;
}

function xorChecksum(bytes: number[]): number {
  return bytes.reduce((acc, byte) => acc ^ byte, 0);
}

export function parseJt808Wire(hexText: string): ParsedJt808Wire | Jt808ParseError {
  const compact = stripInlineAsciiWhitespace(hexText);
  if (!HEX_DIGIT_PATTERN.test(compact)) {
    return { ok: false, error: "包含非十六进制字符" };
  }
  const bytes = hexToBytes(compact);
  if (bytes.length < 2 || bytes[0] !== 0x7e) {
    return { ok: false, error: "缺少起始标志 0x7E" };
  }
  if (bytes[bytes.length - 1] !== 0x7e) {
    return { ok: false, error: "缺少结束标志 0x7E" };
  }
  const inner = unescapeWire(bytes.slice(1, -1));
  if (inner === null) {
    return { ok: false, error: "转义序列无效" };
  }
  if (inner.length < 12) {
    return { ok: false, error: "报文长度不足，帧被截断" };
  }

  const messageId = (inner[0] << 8) | inner[1];
  const bodyProperty = (inner[2] << 8) | inner[3];
  const bodyLength = bodyProperty & 0x3ff;
  const encryption = (bodyProperty >> 10) & 0x07;
  const subpackage = ((bodyProperty >> 13) & 0x01) === 1;
  const versionBits = (bodyProperty >> 14) & 0x03;
  const terminal = bcdDigits(inner.slice(4, 10));
  const serial = (inner[10] << 8) | inner[11];

  let offset = 12;
  let total: number | undefined;
  let index: number | undefined;
  if (subpackage) {
    if (inner.length < offset + 4) {
      return { ok: false, error: "分包信息缺失，帧被截断" };
    }
    total = (inner[offset] << 8) | inner[offset + 1];
    index = (inner[offset + 2] << 8) | inner[offset + 3];
    offset += 4;
  }

  const frameEnd = offset + bodyLength;
  if (inner.length < frameEnd + 1) {
    return { ok: false, error: "报文长度不足，帧被截断" };
  }
  if (inner.length > frameEnd + 1) {
    return { ok: false, error: "帧结束后存在多余数据" };
  }

  const checksum = inner[frameEnd];
  if (xorChecksum(inner.slice(0, frameEnd)) !== checksum) {
    return { ok: false, error: "校验和不匹配" };
  }

  return {
    ok: true,
    header: {
      messageId,
      bodyProperty,
      bodyLength,
      encryption,
      subpackage,
      versionBits,
      terminal,
      serial,
      total,
      index,
      body: inner.slice(offset, frameEnd),
      checksum,
    },
  };
}

export function bytesToHex(bytes: number[]): string {
  return bytes.map((byte) => byte.toString(16).toUpperCase().padStart(2, "0")).join("");
}

export function splitWireFrameHexes(hexText: string): string[] {
  const compact = stripInlineAsciiWhitespace(hexText);
  if (!HEX_DIGIT_PATTERN.test(compact)) {
    return [];
  }
  const bytes = hexToBytes(compact);
  const frames: string[] = [];
  let start = -1;
  for (let index = 0; index < bytes.length; index += 1) {
    if (bytes[index] !== 0x7e) continue;
    if (start !== -1) {
      frames.push(bytesToHex(bytes.slice(start, index + 1)));
      start = -1;
    } else {
      start = index;
    }
  }
  return frames;
}

export function hexWord(value: number): string {
  return `0x${value.toString(16).toUpperCase().padStart(4, "0")}`;
}

export function bcdTime(bytes: number[]): string {
  const [year, month, day, hour, minute, second] = bcdDigits(bytes).match(/.{2}/g) ?? [];
  return `20${year}-${month}-${day} ${hour}:${minute}:${second}`;
}

const ENCRYPTION_LABELS: Record<number, string> = {
  0: "无",
  1: "RSA",
  2: "AES",
};

export function buildJt808FrameTree(
  header: ParsedJt808Header,
  version: Jt808Version,
): ResultNode[] {
  const tree: ResultNode[] = [
    {
      label: "帧结构",
      children: [
        { label: "起始标志", value: "0x7E" },
        { label: "消息 ID", value: hexWord(header.messageId) },
        {
          label: "消息体属性",
          children: [
            { label: "消息体长度", value: String(header.bodyLength) },
            {
              label: "加密方式",
              value: ENCRYPTION_LABELS[header.encryption] ?? hexWord(header.encryption),
            },
            { label: "分包", value: header.subpackage ? "是" : "否" },
            { label: "版本", value: version },
          ],
        },
        { label: "终端手机号", value: header.terminal },
        { label: "消息流水号", value: String(header.serial) },
      ],
    },
  ];
  if (header.subpackage) {
    tree[0].children?.push({
      label: "分包信息",
      children: [
        { label: "总包数", value: String(header.total) },
        { label: "包序号", value: String(header.index) },
      ],
    });
  }
  tree[0].children?.push(
    { label: "校验和", value: `0x${header.checksum.toString(16).toUpperCase().padStart(2, "0")}` },
    { label: "结束标志", value: "0x7E" },
  );
  return tree;
}