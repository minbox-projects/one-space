import type { ParsedJt808Header } from "./frame";

function readUint32(bytes: number[], offset: number): number {
  return (
    ((bytes[offset] ?? 0) << 24) |
    ((bytes[offset + 1] ?? 0) << 16) |
    ((bytes[offset + 2] ?? 0) << 8) |
    (bytes[offset + 3] ?? 0)
  );
}

function readUint16(bytes: number[], offset: number): number {
  return ((bytes[offset] ?? 0) << 8) | (bytes[offset + 1] ?? 0);
}

function readUint8(bytes: number[], offset: number): number {
  return bytes[offset] ?? 0;
}

function hex2(value: number): string {
  return value.toString(16).toUpperCase().padStart(2, "0");
}

function hex4(value: number): string {
  return value.toString(16).toUpperCase().padStart(4, "0");
}

function bytesToHex(bytes: number[]): string {
  return bytes.map((byte) => byte.toString(16).toUpperCase().padStart(2, "0")).join("");
}

function binary16(value: number): string {
  return value.toString(2).padStart(16, "0");
}

function binary32(value: number): string {
  return value.toString(2).padStart(32, "0");
}

function sliceBits(value: number, high: number, low: number): number {
  return (value >> low) & ((1 << (high - low + 1)) - 1);
}

function versionLabel(bits: number): string {
  switch (bits) {
    case 0b10:
    case 0b11:
      return "JTT2019";
    default:
      return "JTT2013";
  }
}

function encryptionLabel(value: number): string {
  switch (value) {
    case 0:
      return "None";
    case 1:
      return "RSA";
    case 2:
      return "AES";
    default:
      return hex2(value);
  }
}

function bodyPropertyObject(header: ParsedJt808Header): Record<string, unknown> {
  return {
    [`[${binary16(header.bodyProperty)}]消息体属性`]: header.bodyProperty,
    版本号: versionLabel(header.versionBits),
    "[bit15]保留": (header.bodyProperty >> 15) & 1,
    "[bit14]保留": (header.bodyProperty >> 14) & 1,
    "[bit13]是否分包": header.subpackage,
    "[bit10~bit12]数据加密": encryptionLabel(header.encryption),
    "[bit0~bit9]消息体长度": header.bodyLength,
  };
}

type BitGroup = { high: number; low: number; label: string };

const ALARM_BIT_GROUPS: BitGroup[] = [
  { high: 31, low: 31, label: "非法开门报警" },
  { high: 30, low: 30, label: "侧翻预警" },
  { high: 29, low: 29, label: "碰撞预警" },
  { high: 28, low: 28, label: "车辆非法位移" },
  { high: 27, low: 27, label: "车辆非法点火" },
  { high: 26, low: 26, label: "车辆被盗(通过车辆防盗器)" },
  { high: 25, low: 25, label: "车辆油量异常" },
  { high: 24, low: 24, label: "车辆VSS故障" },
  { high: 23, low: 23, label: "路线偏离报警" },
  { high: 22, low: 22, label: "路段行驶时间不足/过长" },
  { high: 21, low: 21, label: "进出路线" },
  { high: 20, low: 20, label: "进出区域" },
  { high: 19, low: 19, label: "超时停车" },
  { high: 18, low: 18, label: "当天累计驾驶超时" },
  { high: 17, low: 15, label: "保留" },
  { high: 14, low: 14, label: "疲劳驾驶预警" },
  { high: 13, low: 13, label: "超速预警" },
  { high: 12, low: 12, label: "道路运输证IC卡模块故障" },
  { high: 11, low: 11, label: "摄像头故障" },
  { high: 10, low: 10, label: "TTS模块故障" },
  { high: 9, low: 9, label: "终端LCD或显示器故障" },
  { high: 8, low: 8, label: "终端主电源掉电" },
  { high: 7, low: 7, label: "终端主电源欠压" },
  { high: 6, low: 6, label: "GNSS天线短路" },
  { high: 5, low: 5, label: "GNSS天线未接或被剪断" },
  { high: 4, low: 4, label: "GNSS模块发生故障" },
  { high: 3, low: 3, label: "危险预警" },
  { high: 2, low: 2, label: "疲劳驾驶" },
  { high: 1, low: 1, label: "超速报警" },
  { high: 0, low: 0, label: "紧急报警,触动报警开关后触发" },
];

function alarmObject(alarm: number): Record<string, string> {
  const object: Record<string, string> = {};
  for (const group of ALARM_BIT_GROUPS) {
    const width = group.high - group.low + 1;
    const key =
      width === 1
        ? `[bit${group.high}]${group.label}`
        : `[bit${group.low}~bit${group.high}]${group.label}`;
    object[key] = sliceBits(alarm, group.high, group.low)
      .toString(2)
      .padStart(width, "0");
  }
  return object;
}

type StatusGroup =
  | { kind: "reserved"; low: number; high: number }
  | { kind: "value"; low: number; high: number; label: (value: number) => string };

const STATUS_GROUPS: StatusGroup[] = [
  { kind: "reserved", low: 22, high: 31 },
  {
    kind: "value",
    low: 21,
    high: 21,
    label: (value) => (value === 0 ? "未使用Galileo卫星进行定位" : "使用Galileo卫星进行定位"),
  },
  {
    kind: "value",
    low: 20,
    high: 20,
    label: (value) => (value === 0 ? "未使用GLONASS卫星进行定位" : "使用GLONASS卫星进行定位"),
  },
  {
    kind: "value",
    low: 19,
    high: 19,
    label: (value) => (value === 0 ? "未使用北斗卫星进行定位" : "使用北斗卫星进行定位"),
  },
  {
    kind: "value",
    low: 18,
    high: 18,
    label: (value) => (value === 0 ? "未使用GPS卫星进行定位" : "使用GPS卫星进行定位"),
  },
  { kind: "value", low: 17, high: 17, label: (value) => (value === 0 ? "门5关" : "门5开") },
  { kind: "value", low: 16, high: 16, label: (value) => (value === 0 ? "门4关" : "门4开") },
  { kind: "value", low: 15, high: 15, label: (value) => (value === 0 ? "门3关" : "门3开") },
  { kind: "value", low: 14, high: 14, label: (value) => (value === 0 ? "门2关" : "门2开") },
  { kind: "value", low: 13, high: 13, label: (value) => (value === 0 ? "门1关" : "门1开") },
  { kind: "value", low: 12, high: 12, label: (value) => (value === 0 ? "车门解锁" : "车门锁定") },
  { kind: "value", low: 11, high: 11, label: (value) => (value === 0 ? "车辆电路正常" : "车辆电路故障") },
  { kind: "value", low: 10, high: 10, label: (value) => (value === 0 ? "车辆油路正常" : "车辆油路故障") },
  {
    kind: "value",
    low: 8,
    high: 9,
    label: (value) => (value === 0 ? "空车" : value === 1 ? "半载" : value === 2 ? "满载" : `未知(${value})`),
  },
  { kind: "reserved", low: 6, high: 7 },
  {
    kind: "value",
    low: 5,
    high: 5,
    label: (value) => (value === 0 ? "经纬度未经保密插件加密" : "经纬度经过保密插件加密"),
  },
  { kind: "value", low: 4, high: 4, label: (value) => (value === 0 ? "运营状态" : "停运状态") },
  { kind: "value", low: 3, high: 3, label: (value) => (value === 0 ? "东经" : "西经") },
  { kind: "value", low: 2, high: 2, label: (value) => (value === 0 ? "北纬" : "南纬") },
  { kind: "value", low: 1, high: 1, label: (value) => (value === 0 ? "未定位" : "定位") },
  { kind: "value", low: 0, high: 0, label: (value) => (value === 0 ? "ACC关" : "ACC开") },
];

function statusObject(status: number): Record<string, string> {
  const object: Record<string, string> = {};
  for (const group of STATUS_GROUPS) {
    const width = group.high - group.low + 1;
    const value = sliceBits(status, group.high, group.low);
    if (group.kind === "reserved") {
      object[`[bit${group.low}~bit${group.high}]保留`] = value.toString(2).padStart(width, "0");
    } else {
      const key =
        width === 1
          ? `[${value}]bit${group.low}`
          : `[${value.toString(2).padStart(width, "0")}]bit${group.low}~bit${group.high}`;
      object[key] = group.label(value);
    }
  }
  return object;
}

const EXTENDED_SIGNAL_BITS: Array<{ bit: number; label: string }> = [
  { bit: 14, label: "离合器状态" },
  { bit: 13, label: "加热器工作" },
  { bit: 12, label: "ABS工作" },
  { bit: 11, label: "缓速器工作" },
  { bit: 10, label: "空挡信号" },
  { bit: 9, label: "空调状态" },
  { bit: 8, label: "喇叭信号" },
  { bit: 7, label: "示廓灯" },
  { bit: 6, label: "雾灯信号" },
  { bit: 5, label: "倒档信号" },
  { bit: 4, label: "制动信号" },
  { bit: 3, label: "左转向灯信号" },
  { bit: 2, label: "右转向灯信号" },
  { bit: 1, label: "远光灯信号" },
  { bit: 0, label: "近光灯信号" },
];

function extendedSignalObject(value: number): Record<string, string> {
  const object: Record<string, string> = {
    值: binary32(value),
    "bit15~31": "保留",
  };
  for (const entry of EXTENDED_SIGNAL_BITS) {
    object[`bit${entry.bit}-${entry.label}`] = ((value >> entry.bit) & 1) === 0 ? "无" : "有";
  }
  return object;
}

function ioStatusObject(value: number): Record<string, string> {
  return {
    值: value.toString(2).padStart(16, "0"),
    "bit2~15": "保留",
    bit1: ((value >> 1) & 1) === 0 ? "无" : "有",
    bit0: (value & 1) === 0 ? "无" : "有",
  };
}

type AdditionalInfoEntry = { id: number; length: number; data: number[] };

function parseAdditionalInfos(body: number[], offset: number): AdditionalInfoEntry[] {
  const entries: AdditionalInfoEntry[] = [];
  while (offset + 2 <= body.length) {
    const id = body[offset];
    const length = body[offset + 1];
    const dataStart = offset + 2;
    const dataEnd = dataStart + length;
    if (dataEnd > body.length) break;
    entries.push({ id, length, data: body.slice(dataStart, dataEnd) });
    offset = dataEnd;
  }
  return entries;
}

function additionalInfoObject(id: number, length: number, data: number[]): Record<string, unknown> {
  const idHex = hex2(id);
  const lengthHex = hex2(length);
  switch (id) {
    case 0x01:
      return {
        [`[${idHex}]附加信息Id`]: id,
        [`[${lengthHex}]附加信息长度`]: length,
        [`[${bytesToHex(data)}]里程`]: readUint32(data, 0),
      };
    case 0x03:
      return {
        [`[${idHex}]附加信息Id`]: id,
        [`[${lengthHex}]附加信息长度`]: length,
        [`[${bytesToHex(data)}]速度`]: readUint16(data, 0),
      };
    case 0x25: {
      const value = readUint32(data, 0);
      return {
        [`[${idHex}]附加信息Id`]: id,
        [`[${lengthHex}]附加信息长度`]: length,
        [`[${bytesToHex(data)}]扩展车辆信号状态位`]: value,
        扩展车辆信号状态位对象信息: extendedSignalObject(value),
      };
    }
    case 0x2a: {
      const value = readUint16(data, 0);
      return {
        [`[${idHex}]附加信息Id`]: id,
        [`[${lengthHex}]附加信息长度`]: length,
        [`[${bytesToHex(data)}]IO状态位`]: value,
        IO状态位对象信息: ioStatusObject(value),
      };
    }
    case 0x2b:
      return {
        [`[${idHex}]附加信息Id`]: id,
        [`[${lengthHex}]附加信息长度`]: length,
        [`[${bytesToHex(data.slice(0, 2))}]模拟量通道1`]: readUint16(data, 0),
        [`[${bytesToHex(data.slice(2, 4))}]模拟量通道2`]: readUint16(data, 2),
      };
    case 0x30:
      return {
        [`[${idHex}]附加信息Id`]: id,
        [`[${lengthHex}]附加信息长度`]: length,
        [`[${bytesToHex(data)}]无线通信网络信号强度`]: readUint8(data, 0),
      };
    case 0x31:
      return {
        [`[${idHex}]附加信息Id`]: id,
        [`[${lengthHex}]附加信息长度`]: length,
        [`[${bytesToHex(data)}]GNSS定位卫星数`]: readUint8(data, 0),
      };
    case 0x52:
      return {
        [`[${idHex}]未知附加信息Id`]: id,
        [`[${lengthHex}]未知附加信息长度`]: length,
        "未知附加信息[异常解析]": bytesToHex(data),
      };
    default:
      return {
        [`[${idHex}]未知附加信息Id`]: id,
        [`[${lengthHex}]未知附加信息长度`]: length,
        未知附加信息: `${idHex}${lengthHex}${bytesToHex(data)}`,
      };
  }
}

function bcdTimeString(bytes: number[]): string {
  let digits = "";
  for (const byte of bytes) {
    digits += ((byte >> 4) & 0x0f).toString();
    digits += (byte & 0x0f).toString();
  }
  const parts = digits.match(/.{2}/g) ?? [];
  const [year, month, day, hour, minute, second] = parts;
  return `20${year}-${month}-${day} ${hour}:${minute}:${second}`;
}

function formatPositionTime(bytes: number[]): string {
  for (const byte of bytes) {
    if (((byte >> 4) & 0x0f) > 9 || (byte & 0x0f) > 9) {
      return bytesToHex(bytes);
    }
  }
  return bcdTimeString(bytes);
}

function buildFrameJson(
  header: ParsedJt808Header,
  dataBody: Record<string, unknown>,
): Record<string, unknown> {
  return {
    "[7E]开始": 0x7e,
    [`[${hex4(header.messageId)}]消息Id`]: header.messageId,
    消息体属性对象: bodyPropertyObject(header),
    [`[${header.terminal}]终端手机号`]: header.terminal,
    [`[${hex4(header.serial)}]消息流水号`]: header.serial,
    数据体对象: dataBody,
    [`[${hex2(header.checksum)}]校验码`]: header.checksum,
    "[7E]结束": 0x7e,
  };
}

export function buildJt808PositionJson(header: ParsedJt808Header): Record<string, unknown> {
  const body = header.body;
  const alarm = readUint32(body, 0);
  const status = readUint32(body, 4);
  const timeBytes = body.slice(22, 28);
  const dataBody: Record<string, unknown> = {
    位置信息汇报: bytesToHex(body),
    [`[${binary32(alarm)}]报警标志`]: alarm,
    报警标志对象: alarmObject(alarm),
    [`[${binary32(status)}]状态位标志`]: status,
    状态标志对象: statusObject(status),
    [`[${bytesToHex(body.slice(8, 12))}]纬度`]: readUint32(body, 8),
    [`[${bytesToHex(body.slice(12, 16))}]经度`]: readUint32(body, 12),
    [`[${bytesToHex(body.slice(16, 18))}]高程`]: readUint16(body, 16),
    [`[${bytesToHex(body.slice(18, 20))}]速度`]: readUint16(body, 18),
    [`[${bytesToHex(body.slice(20, 22))}]方向`]: readUint16(body, 20),
    [`[${bytesToHex(timeBytes)}]定位时间`]: formatPositionTime(timeBytes),
    附加信息列表: parseAdditionalInfos(body, 28).map((entry) =>
      additionalInfoObject(entry.id, entry.length, entry.data),
    ),
  };
  return buildFrameJson(header, dataBody);
}

export function buildJt808AnswerJson(header: ParsedJt808Header): Record<string, unknown> {
  const body = header.body;
  const replySerial = readUint16(body, 0);
  const replyId = readUint16(body, 2);
  const result = readUint8(body, 4);
  const dataBody: Record<string, unknown> = {
    平台通用应答: bytesToHex(body),
    [`[${hex4(replySerial)}]应答流水号`]: replySerial,
    [`[${hex4(replyId)}]应答消息Id`]: replyId,
    [`[${hex2(result)}]结果`]: result,
  };
  return buildFrameJson(header, dataBody);
}