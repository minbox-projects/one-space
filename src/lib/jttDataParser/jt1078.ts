import type {
  AnalysisRecord,
  Jt1078Direction,
  Jt1078Operation,
  ResultNode,
} from "./types";
import {
  bcdTime,
  buildJt808FrameTree,
  bytesToHex,
  hexWord,
  parseJt808Wire,
} from "./frame";
import { trimAsciiWhitespace } from "./lexing";

export const JT1078_OPERATIONS: Jt1078Operation[] = [
  "0x9101",
  "0x9102",
  "0x9205",
  "0x9206",
];

export const JT1078_DIRECTIONS: Jt1078Direction[] = ["upstream", "downstream"];

const OPERATION_NAMES: Record<Jt1078Operation, string> = {
  "0x9101": "实时音视频传输请求",
  "0x9102": "音视频实时传输控制",
  "0x9205": "查询音视频资源列表",
  "0x9206": "文件上传指令",
};

const OPERATION_DIRECTIONS: Record<Jt1078Operation, Jt1078Direction> = {
  "0x9101": "downstream",
  "0x9102": "downstream",
  "0x9205": "downstream",
  "0x9206": "downstream",
};

const DIRECTION_LABELS: Record<Jt1078Direction, string> = {
  upstream: "上行",
  downstream: "下行 (平台下发)",
};

const DIRECTION_ERROR_LABELS: Record<Jt1078Direction, string> = {
  upstream: "上行",
  downstream: "下行",
};

function readTime(bytes: number[]): string {
  return bcdTime(bytes);
}

function readText(bytes: number[], offset: number, length: number): { text: string; end: number } {
  return {
    text: bytes
      .slice(offset, offset + length)
      .map((byte) => String.fromCharCode(byte))
      .join(""),
    end: offset + length,
  };
}

export function jt1078BodyNode(
  messageId: Jt1078Operation,
  body: number[],
): ResultNode {
  const name = OPERATION_NAMES[messageId];
  const node: ResultNode = { label: `协议体 (${messageId} ${name})`, children: [] };

  if (messageId === "0x9101") {
    const addressLength = body[0] ?? 0;
    const address = readText(body, 1, addressLength);
    const tcpPort = ((body[address.end] ?? 0) << 8) | (body[address.end + 1] ?? 0);
    const udpPort = ((body[address.end + 2] ?? 0) << 8) | (body[address.end + 3] ?? 0);
    node.children?.push(
      { label: "服务器地址长度", value: String(addressLength) },
      { label: "服务器地址", value: address.text },
      { label: "服务器端口 (TCP)", value: String(tcpPort) },
      { label: "服务器端口 (UDP)", value: String(udpPort) },
      { label: "逻辑通道号", value: String(body[address.end + 4] ?? 0) },
      { label: "数据类型", value: hexByte(body[address.end + 5] ?? 0) },
      { label: "码流类型", value: hexByte(body[address.end + 6] ?? 0) },
      { label: "时间", value: readTime(body.slice(address.end + 7, address.end + 13)) },
    );
    return node;
  }

  if (messageId === "0x9102") {
    node.children?.push(
      { label: "逻辑通道号", value: String(body[0] ?? 0) },
      { label: "控制指令", value: hexByte(body[1] ?? 0) },
      { label: "关闭音视频类型", value: hexByte(body[2] ?? 0) },
      { label: "切换码流类型", value: hexByte(body[3] ?? 0) },
    );
    return node;
  }

  if (messageId === "0x9205") {
    node.children?.push(
      { label: "逻辑通道号", value: String(body[0] ?? 0) },
      { label: "开始时间", value: readTime(body.slice(1, 7)) },
      { label: "结束时间", value: readTime(body.slice(7, 13)) },
    );
    return node;
  }

  const addressLength = body[0] ?? 0;
  const address = readText(body, 1, addressLength);
  const port = ((body[address.end] ?? 0) << 8) | (body[address.end + 1] ?? 0);
  const userLength = body[address.end + 2] ?? 0;
  const user = readText(body, address.end + 3, userLength);
  const passwordLength = body[user.end] ?? 0;
  const password = readText(body, user.end + 1, passwordLength);
  const cursor = password.end;
  node.children?.push(
    { label: "服务器地址长度", value: String(addressLength) },
    { label: "服务器地址", value: address.text },
    { label: "端口", value: String(port) },
    { label: "用户名长度", value: String(userLength) },
    { label: "用户名", value: user.text },
    { label: "密码长度", value: String(passwordLength) },
    { label: "密码", value: password.text },
    { label: "文件上传方式", value: hexByte(body[cursor] ?? 0) },
    { label: "逻辑通道号", value: String(body[cursor + 1] ?? 0) },
    { label: "开始时间", value: readTime(body.slice(cursor + 2, cursor + 8)) },
    { label: "结束时间", value: readTime(body.slice(cursor + 8, cursor + 14)) },
    { label: "报警标志", value: hexWord4(body, cursor + 14) },
    { label: "音视频资源类型", value: hexByte(body[cursor + 18] ?? 0) },
    { label: "文件上传任务 ID", value: String(readUint32(body, cursor + 19)) },
  );
  return node;
}

function hexByte(byte: number): string {
  return `0x${byte.toString(16).toUpperCase().padStart(2, "0")}`;
}

function hexWord4(bytes: number[], offset: number): string {
  const value =
    ((bytes[offset] ?? 0) << 24) |
    ((bytes[offset + 1] ?? 0) << 16) |
    ((bytes[offset + 2] ?? 0) << 8) |
    (bytes[offset + 3] ?? 0);
  return `0x${value.toString(16).toUpperCase().padStart(8, "0")}`;
}

function readUint32(bytes: number[], offset: number): number {
  return (
    ((bytes[offset] ?? 0) << 24) |
    ((bytes[offset + 1] ?? 0) << 16) |
    ((bytes[offset + 2] ?? 0) << 8) |
    (bytes[offset + 3] ?? 0)
  );
}

export function analyzeJt1078(
  input: string,
  operation: Jt1078Operation,
  direction: Jt1078Direction,
): AnalysisRecord {
  const trimmed = trimAsciiWhitespace(input);
  if (trimmed === "") {
    return { kind: "error", error: "输入为空", tree: [] };
  }
  if (trimmed.includes("\n")) {
    return { kind: "error", error: "仅支持单条报文，输入不能包含换行", tree: [] };
  }

  const parsed = parseJt808Wire(trimmed);
  if (!parsed.ok) {
    return { kind: "error", error: parsed.error, tree: [] };
  }

  const { header } = parsed;
  const tree = buildJt808FrameTree(header, "2013");

  const supportedMessageIds = new Set(JT1078_OPERATIONS.map((op) => parseInt(op, 16)));
  if (!supportedMessageIds.has(header.messageId)) {
    tree.push(jt1078UnsupportedBodyNode(header.body));
    return { kind: "unsupported", tree };
  }

  if (header.messageId !== parseInt(operation, 16)) {
    return {
      kind: "error",
      error: `报文消息 ID ${hexWord(header.messageId)} 与所选操作 ${operation} 不匹配`,
      tree,
    };
  }

  const requiredDirection = OPERATION_DIRECTIONS[operation];
  if (direction !== requiredDirection) {
    return {
      kind: "error",
      error: `所选方向(${DIRECTION_ERROR_LABELS[direction]})与操作 ${operation} 的公开方向(${DIRECTION_ERROR_LABELS[requiredDirection]})不匹配`,
      tree,
    };
  }

  tree.push({ label: "传输方向", value: DIRECTION_LABELS[direction] });
  tree.push(jt1078BodyNode(operation, header.body));
  return { kind: "success", tree };
}

export function jt1078UnsupportedBodyNode(body: number[]): ResultNode {
  return {
    label: "协议体",
    children: [
      { label: "原始数据体 (Hex)", value: bytesToHex(body) },
      { label: "支持状态", value: "不在本模式冻结支持范围内" },
    ],
  };
}