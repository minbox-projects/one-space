import { describe, expect, it } from "vitest";
import { analyzeJt808 } from "./jt808";
import { buildJt808PositionJson } from "./jt808Json";
import { JT808_ANSWER_8001, JT808_POSITION_0200, JT808_POSITION_0704 } from "./fixtures";
import { parseJt808Wire } from "./frame";

const REFERENCE_JSON = `{
  "[7E]开始": 126,
  "[0200]消息Id": 512,
  "消息体属性对象": {
    "[0000000001100000]消息体属性": 96,
    "版本号": "JTT2013",
    "[bit15]保留": 0,
    "[bit14]保留": 0,
    "[bit13]是否分包": false,
    "[bit10~bit12]数据加密": "None",
    "[bit0~bit9]消息体长度": 96
  },
  "[018920259024]终端手机号": "018920259024",
  "[00F7]消息流水号": 247,
  "数据体对象": {
    "位置信息汇报": "00000000000C000301D1F6F206F9DAAD0029000000F326090415180601040001BBF0030204381404000000001504000000001604000000001702000018030000001904000000002504000000002A0200002B040000000030011931010F520100",
    "[00000000000000000000000000000000]报警标志": 0,
    "报警标志对象": {
      "[bit31]非法开门报警": "0",
      "[bit30]侧翻预警": "0",
      "[bit29]碰撞预警": "0",
      "[bit28]车辆非法位移": "0",
      "[bit27]车辆非法点火": "0",
      "[bit26]车辆被盗(通过车辆防盗器)": "0",
      "[bit25]车辆油量异常": "0",
      "[bit24]车辆VSS故障": "0",
      "[bit23]路线偏离报警": "0",
      "[bit22]路段行驶时间不足/过长": "0",
      "[bit21]进出路线": "0",
      "[bit20]进出区域": "0",
      "[bit19]超时停车": "0",
      "[bit18]当天累计驾驶超时": "0",
      "[bit15~bit17]保留": "000",
      "[bit14]疲劳驾驶预警": "0",
      "[bit13]超速预警": "0",
      "[bit12]道路运输证IC卡模块故障": "0",
      "[bit11]摄像头故障": "0",
      "[bit10]TTS模块故障": "0",
      "[bit9]终端LCD或显示器故障": "0",
      "[bit8]终端主电源掉电": "0",
      "[bit7]终端主电源欠压": "0",
      "[bit6]GNSS天线短路": "0",
      "[bit5]GNSS天线未接或被剪断": "0",
      "[bit4]GNSS模块发生故障": "0",
      "[bit3]危险预警": "0",
      "[bit2]疲劳驾驶": "0",
      "[bit1]超速报警": "0",
      "[bit0]紧急报警,触动报警开关后触发": "0"
    },
    "[00000000000011000000000000000011]状态位标志": 786435,
    "状态标志对象": {
      "[bit22~bit31]保留": "0000000000",
      "[0]bit21": "未使用Galileo卫星进行定位",
      "[0]bit20": "未使用GLONASS卫星进行定位",
      "[1]bit19": "使用北斗卫星进行定位",
      "[1]bit18": "使用GPS卫星进行定位",
      "[0]bit17": "门5关",
      "[0]bit16": "门4关",
      "[0]bit15": "门3关",
      "[0]bit14": "门2关",
      "[0]bit13": "门1关",
      "[0]bit12": "车门解锁",
      "[0]bit11": "车辆电路正常",
      "[0]bit10": "车辆油路正常",
      "[00]bit8~bit9": "空车",
      "[bit6~bit7]保留": "00",
      "[0]bit5": "经纬度未经保密插件加密",
      "[0]bit4": "运营状态",
      "[0]bit3": "东经",
      "[0]bit2": "北纬",
      "[1]bit1": "定位",
      "[1]bit0": "ACC开"
    },
    "[01D1F6F2]纬度": 30537458,
    "[06F9DAAD]经度": 117037741,
    "[0029]高程": 41,
    "[0000]速度": 0,
    "[00F3]方向": 243,
    "[260904151806]定位时间": "2026-09-04 15:18:06",
    "附加信息列表": [
      {
        "[01]附加信息Id": 1,
        "[04]附加信息长度": 4,
        "[0001BBF0]里程": 113648
      },
      {
        "[03]附加信息Id": 3,
        "[02]附加信息长度": 2,
        "[0438]速度": 1080
      },
      {
        "[14]未知附加信息Id": 20,
        "[04]未知附加信息长度": 4,
        "未知附加信息": "140400000000"
      },
      {
        "[15]未知附加信息Id": 21,
        "[04]未知附加信息长度": 4,
        "未知附加信息": "150400000000"
      },
      {
        "[16]未知附加信息Id": 22,
        "[04]未知附加信息长度": 4,
        "未知附加信息": "160400000000"
      },
      {
        "[17]未知附加信息Id": 23,
        "[02]未知附加信息长度": 2,
        "未知附加信息": "17020000"
      },
      {
        "[18]未知附加信息Id": 24,
        "[03]未知附加信息长度": 3,
        "未知附加信息": "1803000000"
      },
      {
        "[19]未知附加信息Id": 25,
        "[04]未知附加信息长度": 4,
        "未知附加信息": "190400000000"
      },
      {
        "[25]附加信息Id": 37,
        "[04]附加信息长度": 4,
        "[00000000]扩展车辆信号状态位": 0,
        "扩展车辆信号状态位对象信息": {
          "值": "00000000000000000000000000000000",
          "bit15~31": "保留",
          "bit14-离合器状态": "无",
          "bit13-加热器工作": "无",
          "bit12-ABS工作": "无",
          "bit11-缓速器工作": "无",
          "bit10-空挡信号": "无",
          "bit9-空调状态": "无",
          "bit8-喇叭信号": "无",
          "bit7-示廓灯": "无",
          "bit6-雾灯信号": "无",
          "bit5-倒档信号": "无",
          "bit4-制动信号": "无",
          "bit3-左转向灯信号": "无",
          "bit2-右转向灯信号": "无",
          "bit1-远光灯信号": "无",
          "bit0-近光灯信号": "无"
        }
      },
      {
        "[2A]附加信息Id": 42,
        "[02]附加信息长度": 2,
        "[0000]IO状态位": 0,
        "IO状态位对象信息": {
          "值": "0000000000000000",
          "bit2~15": "保留",
          "bit1": "无",
          "bit0": "无"
        }
      },
      {
        "[2B]附加信息Id": 43,
        "[04]附加信息长度": 4,
        "[0000]模拟量通道1": 0,
        "[0000]模拟量通道2": 0
      },
      {
        "[30]附加信息Id": 48,
        "[01]附加信息长度": 1,
        "[19]无线通信网络信号强度": 25
      },
      {
        "[31]附加信息Id": 49,
        "[01]附加信息长度": 1,
        "[0F]GNSS定位卫星数": 15
      },
      {
        "[52]未知附加信息Id": 82,
        "[01]未知附加信息长度": 1,
        "未知附加信息[异常解析]": "00"
      }
    ]
  },
  "[15]校验码": 21,
  "[7E]结束": 126
}`;

describe("buildJt808PositionJson", () => {
  it("renders the 0x0200 position report exactly as the reference JSON", () => {
    const parsed = parseJt808Wire(JT808_POSITION_0200);
    if (!parsed.ok) throw new Error(parsed.error);

    expect(JSON.stringify(buildJt808PositionJson(parsed.header), null, 2)).toBe(REFERENCE_JSON);
  });

  it("reports the 0x0200 frame as a success record carrying the JSON in automatic mode", () => {
    const [record] = analyzeJt808(JT808_POSITION_0200, "automatic");

    expect(record.kind).toBe("success");
    expect(record.line).toBe(1);
    expect(record.json).toEqual(JSON.parse(REFERENCE_JSON));
  });

  it("keeps the 0x0200 body out of the JT1078 extension mode scope", () => {
    const [record] = analyzeJt808(JT808_POSITION_0200, "jt1078-extension");

    expect(record.kind).toBe("unsupported");
    expect(record.json).toBeUndefined();
  });

  it("parses the 0x0704 body as a position report with a raw time field", () => {
    const [record] = analyzeJt808(JT808_POSITION_0704, "automatic");

    expect(record.kind).toBe("success");
    const json = record.json as Record<string, unknown>;
    expect(json["[0704]消息Id"]).toBe(1796);
    expect(json["[028920258605]终端手机号"]).toBe("028920258605");
    const dataBody = json["数据体对象"] as Record<string, unknown>;
    expect(dataBody["位置信息汇报"]).toBe(
      "000101006000000000000C00030232BD3E070B523B0024000000B326062713383701040002258E030200001404000000001504000000001604000000001702000018030000001904000000002504000000002A0200002B040000000030010031011C520100",
    );
    expect(dataBody["[24000000B326]定位时间"]).toBe("24000000B326");
    expect(dataBody["[00000C00]纬度"]).toBe(3072);
    expect(dataBody["[030232BD]经度"]).toBe(50475709);
    const alarmObject = dataBody["报警标志对象"] as Record<string, string>;
    expect(alarmObject["[bit8]终端主电源掉电"]).toBe("1");
    const additional = dataBody["附加信息列表"] as Array<Record<string, unknown>>;
    expect(additional).toHaveLength(2);
    expect(additional[0]["[06]未知附加信息Id"]).toBe(6);
    expect(additional[1]["[00]未知附加信息Id"]).toBe(0);
  });

  it("parses the 0x8001 platform general answer with reply fields", () => {
    const [record] = analyzeJt808(JT808_ANSWER_8001, "automatic");

    expect(record.kind).toBe("success");
    const json = record.json as Record<string, unknown>;
    expect(json["[8001]消息Id"]).toBe(32769);
    expect(json["[7708]消息流水号"]).toBe(30472);
    const dataBody = json["数据体对象"] as Record<string, unknown>;
    expect(dataBody["平台通用应答"]).toBe("00F7020000");
    expect(dataBody["[00F7]应答流水号"]).toBe(247);
    expect(dataBody["[0200]应答消息Id"]).toBe(512);
    expect(dataBody["[00]结果"]).toBe(0);
  });
});