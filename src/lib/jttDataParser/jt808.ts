import type {
  AnalysisRecord,
  Jt808Mode,
  ResultNode,
} from "./types";
import type { Jt808Version, ParsedJt808Header } from "./frame";
import type { Jt1078Operation } from "./types";
import {
  buildJt808FrameTree,
  bytesToHex,
  hexWord,
  parseJt808Wire,
} from "./frame";
import { nonBlankSourceLines } from "./lexing";
import { jt1078BodyNode } from "./jt1078";
import { buildJt808AnswerJson, buildJt808PositionJson } from "./jt808Json";

export const JT808_MODES: Jt808Mode[] = [
  "automatic",
  "jt1078-extension",
  "jiangsu-active-safety",
  "guangdong-active-safety",
  "force-2013",
];

export function jt808Modes(): Jt808Mode[] {
  return [...JT808_MODES];
}

const MEDIA_TYPE_LABELS: Record<number, string> = {
  0: "无",
  1: "图像",
  2: "音频",
  3: "视频",
};

const MEDIA_FORMAT_LABELS: Record<number, string> = {
  0: "无",
  1: "JPEG",
  2: "TIF",
  3: "MP3",
  4: "WAV",
  5: "WMV",
};

const JT1078_MESSAGE_IDS = new Set([0x9101, 0x9102, 0x9205, 0x9206]);
const JIANGSU_ACTIVE_SAFETY_IDS = new Set([0x0b01, 0x0b02]);
const GUANGDONG_ACTIVE_SAFETY_IDS = new Set([0x0a01]);

function recognizeVersion(
  header: ParsedJt808Header,
  mode: Jt808Mode,
): Jt808Version {
  if (mode !== "automatic") return "2013";
  if (header.versionBits === 0b01) return "2013";
  if (header.subpackage) return "2019";
  return "2011";
}

type PackageGroup = {
  key: string;
  totals: Set<number>;
  indexes: number[];
};

function packageKey(version: Jt808Version, header: ParsedJt808Header): string {
  return `${version}|${header.terminal}|${hexWord(header.messageId)}|${header.serial}`;
}

function groupState(group: PackageGroup): string {
  const distinct = [...new Set(group.indexes)];
  if (distinct.length !== group.indexes.length) return "包序号重复";
  if (group.totals.size > 1) return "总包数冲突";
  const total = [...group.totals][0];
  if (total === undefined) return "缺少分包";
  const complete =
    distinct.length === total && distinct.every((index, position) => index === position + 1);
  return complete ? "完整合并" : "缺少分包";
}

function readUint32(bytes: number[], offset: number): number {
  return (
    ((bytes[offset] ?? 0) << 24) |
    ((bytes[offset + 1] ?? 0) << 16) |
    ((bytes[offset + 2] ?? 0) << 8) |
    (bytes[offset + 3] ?? 0)
  );
}

function hexByte(byte: number): string {
  return `0x${byte.toString(16).toUpperCase().padStart(2, "0")}`;
}

function hexDword(value: number): string {
  return `0x${value.toString(16).toUpperCase().padStart(8, "0")}`;
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

export function buildJt808LocationNodes(bytes: number[]): ResultNode[] {
  return [
    { label: "报警标志", value: hexDword(readUint32(bytes, 0)) },
    { label: "状态", value: hexDword(readUint32(bytes, 4)) },
    { label: "经度", value: String(readUint32(bytes, 8)) },
    { label: "纬度", value: String(readUint32(bytes, 12)) },
    { label: "海拔", value: String(((bytes[16] ?? 0) << 8) | (bytes[17] ?? 0)) },
    { label: "速度", value: String(((bytes[18] ?? 0) << 8) | (bytes[19] ?? 0)) },
    { label: "方向", value: String(((bytes[20] ?? 0) << 8) | (bytes[21] ?? 0)) },
    { label: "时间", value: bcdTime(bytes.slice(22, 28)) },
  ];
}

function jt8080801BodyNode(
  header: ParsedJt808Header,
  version: Jt808Version,
): ResultNode {
  const node: ResultNode = { label: "协议体 (0x0801 多媒体上传)", children: [] };
  const body = header.body;

  if (header.subpackage && (header.index ?? 0) > 1) {
    node.children?.push({ label: "分包数据 (Hex)", value: bytesToHex(body) });
    return node;
  }

  if (body.length < 8) {
    node.children?.push({ label: "原始数据体 (Hex)", value: bytesToHex(body) });
    node.children?.push({
      label: "字段解析",
      value: "协议体长度不足 8 字节，无法解析 0x0801 固定字段",
    });
    return node;
  }

  node.children?.push(
    { label: "多媒体 ID", value: String(readUint32(body, 0)) },
    {
      label: "多媒体类型",
      value: `${hexByte(body[4])} (${MEDIA_TYPE_LABELS[body[4]] ?? "未知"})`,
    },
    {
      label: "多媒体格式编码",
      value: `${hexByte(body[5])} (${MEDIA_FORMAT_LABELS[body[5]] ?? "未知"})`,
    },
    { label: "事件项编码", value: hexByte(body[6]) },
    { label: "通道 ID", value: String(body[7]) },
  );

  let offset = 8;
  if (version !== "2011") {
    if (body.length < 36) {
      node.children?.push({ label: "位置信息", value: "协议体长度不足，无法解析位置信息" });
    } else {
      node.children?.push({ label: "位置信息", children: buildJt808LocationNodes(body.slice(8, 36)) });
    }
    offset = 36;
  }

  node.children?.push({ label: "多媒体数据 (Hex)", value: bytesToHex(body.slice(offset)) });
  return node;
}

function jt808PositionBodyNode(header: ParsedJt808Header): ResultNode {
  return {
    label: `协议体 (${hexWord(header.messageId)} 位置信息汇报)`,
    children: [{ label: "原始数据体 (Hex)", value: bytesToHex(header.body) }],
  };
}

function jt808AnswerBodyNode(header: ParsedJt808Header): ResultNode {
  return {
    label: `协议体 (${hexWord(header.messageId)} 平台通用应答)`,
    children: [{ label: "原始数据体 (Hex)", value: bytesToHex(header.body) }],
  };
}

function unsupportedBodyNode(header: ParsedJt808Header): ResultNode {
  return {
    label: "协议体",
    children: [
      { label: "原始数据体 (Hex)", value: bytesToHex(header.body) },
      { label: "支持状态", value: "不在本模式冻结支持范围内" },
    ],
  };
}

function extensionBodyNode(
  messageId: string,
  name: string,
  body: number[],
): ResultNode {
  return {
    label: `协议体 (${messageId} ${name})`,
    children: [{ label: "原始数据体 (Hex)", value: bytesToHex(body) }],
  };
}

function buildBodyNodes(
  header: ParsedJt808Header,
  version: Jt808Version,
  mode: Jt808Mode,
): { kind: "success" | "unsupported"; nodes: ResultNode[]; json?: unknown } {
  if (mode === "automatic" || mode === "force-2013") {
    if (header.messageId === 0x0200 || header.messageId === 0x0704) {
      return {
        kind: "success",
        nodes: [jt808PositionBodyNode(header)],
        json: buildJt808PositionJson(header),
      };
    }
    if (header.messageId === 0x8001) {
      return {
        kind: "success",
        nodes: [jt808AnswerBodyNode(header)],
        json: buildJt808AnswerJson(header),
      };
    }
    if (header.messageId === 0x0801) {
      return { kind: "success", nodes: [jt8080801BodyNode(header, version)] };
    }
    return { kind: "unsupported", nodes: [unsupportedBodyNode(header)] };
  }
  if (mode === "jt1078-extension") {
    if (JT1078_MESSAGE_IDS.has(header.messageId)) {
      const operation = `0x${header.messageId.toString(16).padStart(4, "0")}` as Jt1078Operation;
      return { kind: "success", nodes: [jt1078BodyNode(operation, header.body)] };
    }
    return { kind: "unsupported", nodes: [unsupportedBodyNode(header)] };
  }
  if (mode === "jiangsu-active-safety") {
    if (JIANGSU_ACTIVE_SAFETY_IDS.has(header.messageId)) {
      return {
        kind: "success",
        nodes: [
          extensionBodyNode(hexWord(header.messageId), "江苏主动安全报警", header.body),
        ],
      };
    }
    return { kind: "unsupported", nodes: [unsupportedBodyNode(header)] };
  }
  if (mode === "guangdong-active-safety") {
    if (GUANGDONG_ACTIVE_SAFETY_IDS.has(header.messageId)) {
      return {
        kind: "success",
        nodes: [
          extensionBodyNode(hexWord(header.messageId), "广东主动安全报警", header.body),
        ],
      };
    }
    return { kind: "unsupported", nodes: [unsupportedBodyNode(header)] };
  }
  return { kind: "unsupported", nodes: [unsupportedBodyNode(header)] };
}

export function analyzeJt808(input: string, mode: Jt808Mode): AnalysisRecord[] {
  type RawSuccess = { line: number; version: Jt808Version; header: ParsedJt808Header };
  const errorRecords: AnalysisRecord[] = [];
  const successes: RawSuccess[] = [];
  const groups = new Map<string, PackageGroup>();

  for (const line of nonBlankSourceLines(input)) {
    const parsed = parseJt808Wire(line.text);
    if (!parsed.ok) {
      errorRecords.push({
        kind: "error",
        line: line.lineNumber,
        error: parsed.error,
        tree: [],
      });
      continue;
    }
    const version = recognizeVersion(parsed.header, mode);
    successes.push({ line: line.lineNumber, version, header: parsed.header });
    if (parsed.header.subpackage && parsed.header.total !== undefined) {
      const key = packageKey(version, parsed.header);
      let group = groups.get(key);
      if (!group) {
        group = { key, totals: new Set(), indexes: [] };
        groups.set(key, group);
      }
      group.totals.add(parsed.header.total);
      group.indexes.push(parsed.header.index ?? 0);
    }
  }

  const records: AnalysisRecord[] = [...errorRecords];
  for (const success of successes) {
    const { line, version, header } = success;
    const tree = buildJt808FrameTree(header, version);
    if (header.subpackage) {
      const key = packageKey(version, header);
      const group = groups.get(key);
      const distinctIndexes = group ? [...new Set(group.indexes)].sort((a, b) => a - b) : [];
      tree.push({
        label: "分包组装",
        children: [
          { label: "分组", value: key },
          { label: "声明总包数", value: String(header.total) },
          { label: "当前包序号", value: String(header.index) },
          { label: "已收集包序号", value: `[${distinctIndexes.join(", ")}]` },
          { label: "组装状态", value: group ? groupState(group) : "缺少分包" },
        ],
      });
    }
    const body = buildBodyNodes(header, version, mode);
    tree.push(...body.nodes);
    const record: AnalysisRecord = { kind: body.kind, line, tree };
    if (body.json !== undefined) {
      record.json = body.json;
    }
    records.push(record);
  }

  return records.sort((a, b) => (a.line ?? 0) - (b.line ?? 0));
}