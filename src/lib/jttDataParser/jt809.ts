import type {
  AnalysisRecord,
  Jt809CryptoMode,
  Jt809Uint32Params,
  Jt809Version,
  ResultNode,
} from "./types";
import { buildJt808LocationNodes } from "./jt808";
import { trimAsciiWhitespace } from "./lexing";

export const JT809_VERSIONS: Jt809Version[] = ["2011", "2019"];
export const JT809_CRYPTO_MODES: Jt809CryptoMode[] = ["unencrypted", "encrypted"];

export const JT809_UINT32_MAX = 4294967295;

export type Jt809Uint32ParamErrorKind =
  | "missing"
  | "signed"
  | "fractional"
  | "nonnumeric"
  | "out-of-range";

export type Jt809ParamParseResult =
  | { ok: true; value: number }
  | { ok: false; kind: Jt809Uint32ParamErrorKind };

const PARAM_ERROR_LABELS: Record<Jt809Uint32ParamErrorKind, string> = {
  missing: "参数不能为空",
  signed: "参数必须是无符号整数",
  fractional: "参数必须是整数",
  nonnumeric: "参数必须是十进制整数",
  "out-of-range": "参数超出 uint32 范围 (0-4294967295)",
};

export function parseJt809Uint32Param(raw: string): Jt809ParamParseResult {
  const value = trimAsciiWhitespace(raw);
  if (value === "") return { ok: false, kind: "missing" };
  if (value.includes("-") || value.includes("+")) return { ok: false, kind: "signed" };
  if (value.includes(".")) return { ok: false, kind: "fractional" };
  if (!/^[0-9]+$/.test(value)) return { ok: false, kind: "nonnumeric" };
  const parsed = Number(value);
  if (parsed > JT809_UINT32_MAX) return { ok: false, kind: "out-of-range" };
  return { ok: true, value: parsed };
}

export type Jt809ParamsValidation =
  | { ok: true; values: Jt809Uint32Params }
  | { ok: false; error: string };

export function validateJt809Params(params: {
  m1: string;
  ia1: string;
  ic1: string;
}): Jt809ParamsValidation {
  const result = { ok: true as const, values: {} as Jt809Uint32Params };
  for (const field of ["m1", "ia1", "ic1"] as const) {
    const parsed = parseJt809Uint32Param(params[field]);
    if (!parsed.ok) {
      return {
        ok: false,
        error: `${field.toUpperCase()} ${PARAM_ERROR_LABELS[parsed.kind]}`,
      };
    }
    result.values[field] = parsed.value;
  }
  return result;
}

function hexByte(byte: number): string {
  return `0x${byte.toString(16).toUpperCase().padStart(2, "0")}`;
}

function hexWord(value: number): string {
  return `0x${value.toString(16).toUpperCase().padStart(4, "0")}`;
}

function bcdTime(bytes: number[]): string {
  let digits = "";
  for (const byte of bytes) {
    digits += ((byte >> 4) & 0x0f).toString();
    digits += (byte & 0x0f).toString();
  }
  const parts = digits.match(/.{2}/g) ?? [];
  const [year, month, day, hour, minute, second] = parts;
  return `20${year}-${month}-${day} ${hour}:${minute}:${second}`;
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

function bytesToHex(bytes: number[]): string {
  return bytes.map((byte) => byte.toString(16).toUpperCase().padStart(2, "0")).join("");
}

type Jt809Header = {
  length: number;
  serial: number;
  messageType: number;
  lowerId: number;
  upperId: number;
  time: number[];
  cryptoFlag: number;
  bodyLength: number;
  body: number[];
  crc: number;
};

type ParsedJt809 = { ok: true; header: Jt809Header } | { ok: false; error: string };

function parseJt809(hexText: string): ParsedJt809 {
  const compact = hexText.replace(/[\s]+/g, "");
  if (!/^[0-9A-Fa-f]+$/.test(compact)) {
    return { ok: false, error: "包含非十六进制字符" };
  }
  const bytes: number[] = [];
  for (let index = 0; index < compact.length; index += 2) {
    bytes.push(parseInt(compact.slice(index, index + 2), 16));
  }

  if (bytes.length < 2 || bytes[bytes.length - 2] !== 0x7b || bytes[bytes.length - 1] !== 0x7e) {
    return { ok: false, error: "缺少结束标志 0x7B 0x7E" };
  }

  const length =
    ((bytes[0] ?? 0) << 24) |
    ((bytes[1] ?? 0) << 16) |
    ((bytes[2] ?? 0) << 8) |
    (bytes[3] ?? 0);
  const crcPosition = 4 + length - 2;
  if (bytes.length !== 4 + length + 2) {
    return { ok: false, error: "报文长度与数据不符" };
  }
  if (crcPosition < 22 || crcPosition + 2 > bytes.length) {
    return { ok: false, error: "报文长度不足，帧被截断" };
  }

  const serial = ((bytes[4] << 8) | bytes[5]);
  const messageType = (bytes[6] << 8) | bytes[7];
  const lowerId = ((bytes[8] << 24) | (bytes[9] << 16) | (bytes[10] << 8) | bytes[11]);
  const upperId = ((bytes[12] << 24) | (bytes[13] << 16) | (bytes[14] << 8) | bytes[15]);
  const time = bytes.slice(16, 22);
  const cryptoFlag = bytes[22];
  const bodyLength = ((bytes[23] << 24) | (bytes[24] << 16) | (bytes[25] << 8) | bytes[26]);
  const bodyStart = 27;
  const bodyEnd = bodyStart + bodyLength;
  if (bodyEnd !== crcPosition) {
    return { ok: false, error: "报文长度与数据不符" };
  }
  const body = bytes.slice(bodyStart, bodyEnd);
  const crc = (bytes[crcPosition] << 8) | bytes[crcPosition + 1];
  if (crc16Ccitt(bytes.slice(22, crcPosition)) !== crc) {
    return { ok: false, error: "校验码不匹配" };
  }

  return {
    ok: true,
    header: { length, serial, messageType, lowerId, upperId, time, cryptoFlag, bodyLength, body, crc },
  };
}

function cryptoLabel(flag: number): string {
  if (flag === 0) return "0x00 (不加密)";
  if (flag === 1) return "0x01 (RSA)";
  if (flag === 2) return "0x02 (加密)";
  return hexByte(flag);
}

function frameTree(header: Jt809Header): ResultNode[] {
  return [
    {
      label: "帧结构",
      children: [
        { label: "报文长度", value: String(header.length) },
        { label: "报文序列号", value: String(header.serial) },
        { label: "报文类型", value: hexWord(header.messageType) },
        { label: "下级平台接入码", value: String(header.lowerId) },
        { label: "上级平台接入码", value: String(header.upperId) },
        { label: "日期时间", value: bcdTime(header.time) },
        { label: "加密标识", value: cryptoLabel(header.cryptoFlag) },
        { label: "数据长度", value: String(header.bodyLength) },
        { label: "校验码", value: `0x${header.crc.toString(16).toUpperCase().padStart(4, "0")}` },
        { label: "结束标志", value: "0x7B 0x7E" },
      ],
    },
  ];
}

function vehicleBridgeBodyNode(header: Jt809Header): ResultNode {
  const node: ResultNode = { label: "协议体 (0x0200 车辆实时位置信息)", children: [] };
  const body = header.body;
  let plateEnd = body.indexOf(0x00);
  if (plateEnd === -1) plateEnd = body.length;
  const plate = new TextDecoder("utf-8").decode(Uint8Array.from(body.slice(0, plateEnd)));
  const color = body[plateEnd + 1] ?? 0;
  const locationStart = plateEnd + 2;
  node.children?.push({
    label: "车辆标识",
    children: [
      { label: "车牌号", value: plate },
      { label: "车辆颜色", value: hexByte(color) },
    ],
  });
  node.children?.push({
    label: "位置信息",
    children: buildJt808LocationNodes(body.slice(locationStart, locationStart + 28)),
  });
  return node;
}

function unsupportedBodyNode(header: Jt809Header): ResultNode {
  return {
    label: "协议体",
    children: [
      { label: "原始数据体 (Hex)", value: bytesToHex(header.body) },
      { label: "支持状态", value: "不在本模式冻结支持范围内" },
    ],
  };
}

function encryptedBodyNode(header: Jt809Header): ResultNode {
  return {
    label: "协议体",
    children: [
      {
        label: "加密状态",
        value: "数据体已加密，冻结范围内无可审计的公开解密规范",
      },
      { label: "原始数据体 (Hex)", value: bytesToHex(header.body) },
    ],
  };
}

export function analyzeJt809(
  input: string,
  version: Jt809Version,
  cryptoMode: Jt809CryptoMode,
  params: { m1: string; ia1: string; ic1: string },
): AnalysisRecord {
  const trimmed = trimAsciiWhitespace(input);
  if (trimmed === "") {
    return { kind: "error", error: "输入为空", tree: [] };
  }
  if (trimmed.includes("\n")) {
    return { kind: "error", error: "仅支持单条报文，输入不能包含换行", tree: [] };
  }

  if (cryptoMode === "encrypted") {
    const validation = validateJt809Params(params);
    if (!validation.ok) {
      return { kind: "error", error: validation.error, tree: [] };
    }
  }

  const parsed = parseJt809(trimmed);
  if (!parsed.ok) {
    return { kind: "error", error: parsed.error, tree: [] };
  }

  const { header } = parsed;
  const tree = frameTree(header);

  if (cryptoMode === "encrypted") {
    tree.push(encryptedBodyNode(header));
    return { kind: "success", tree };
  }

  if (version === "2019" && header.messageType === 0x0200) {
    tree.push(vehicleBridgeBodyNode(header));
    return { kind: "success", tree };
  }

  tree.push(unsupportedBodyNode(header));
  return { kind: "unsupported", tree };
}