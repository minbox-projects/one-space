import { describe, expect, it } from "vitest";
import { analyzeJt1078 } from "./jt1078";
import {
  JT1078_0X9101,
  JT1078_0X9102,
  JT1078_0X9205,
  JT1078_0X9206,
  JT1078_UNLISTED_0X0200,
} from "./fixtures";
import { findNodeValue } from "./testUtils";

describe("analyzeJt1078", () => {
  it("parses a downstream 0x9101 realtime stream request", () => {
    const record = analyzeJt1078(JT1078_0X9101, "0x9101", "downstream");

    expect(record.kind).toBe("success");
    expect(record.error).toBeUndefined();
    expect(findNodeValue(record.tree, "消息 ID")).toBe("0x9101");
    expect(findNodeValue(record.tree, "传输方向")).toBe("下行 (平台下发)");
    expect(findNodeValue(record.tree, "服务器地址")).toBe("192.168.1.100");
    expect(findNodeValue(record.tree, "服务器端口 (TCP)")).toBe("8080");
    expect(findNodeValue(record.tree, "服务器端口 (UDP)")).toBe("8081");
    expect(findNodeValue(record.tree, "逻辑通道号")).toBe("1");
    expect(findNodeValue(record.tree, "时间")).toBe("2026-09-04 14:30:00");
    expect(findNodeValue(record.tree, "校验和")).toBe("0x3C");
  });

  it("parses a downstream 0x9102 stream control", () => {
    const record = analyzeJt1078(JT1078_0X9102, "0x9102", "downstream");

    expect(record.kind).toBe("success");
    expect(findNodeValue(record.tree, "控制指令")).toBe("0x01");
    expect(findNodeValue(record.tree, "切换码流类型")).toBe("0x00");
  });

  it("parses a downstream 0x9205 resource list query", () => {
    const record = analyzeJt1078(JT1078_0X9205, "0x9205", "downstream");

    expect(record.kind).toBe("success");
    expect(findNodeValue(record.tree, "开始时间")).toBe("2026-09-04 08:00:00");
    expect(findNodeValue(record.tree, "结束时间")).toBe("2026-09-04 10:30:00");
  });

  it("parses a downstream 0x9206 file upload instruction", () => {
    const record = analyzeJt1078(JT1078_0X9206, "0x9206", "downstream");

    expect(record.kind).toBe("success");
    expect(findNodeValue(record.tree, "服务器地址")).toBe("203.0.113.10");
    expect(findNodeValue(record.tree, "用户名")).toBe("camera");
    expect(findNodeValue(record.tree, "密码")).toBe("pass123");
    expect(findNodeValue(record.tree, "报警标志")).toBe("0x00000001");
    expect(findNodeValue(record.tree, "文件上传任务 ID")).toBe("1");
  });

  it("rejects an upstream direction for a downstream-only operation", () => {
    const record = analyzeJt1078(JT1078_0X9101, "0x9101", "upstream");

    expect(record.kind).toBe("error");
    expect(record.error).toBe("所选方向(上行)与操作 0x9101 的公开方向(下行)不匹配");
  });

  it("rejects a packet whose message ID does not match the selected operation", () => {
    const record = analyzeJt1078(JT1078_0X9102, "0x9101", "downstream");

    expect(record.kind).toBe("error");
    expect(record.error).toBe("报文消息 ID 0x9102 与所选操作 0x9101 不匹配");
  });

  it("reports an unlisted body as unsupported while keeping frame fields", () => {
    const record = analyzeJt1078(JT1078_UNLISTED_0X0200, "0x9101", "downstream");

    expect(record.kind).toBe("unsupported");
    expect(findNodeValue(record.tree, "消息 ID")).toBe("0x0200");
    expect(findNodeValue(record.tree, "支持状态")).toBe("不在本模式冻结支持范围内");
  });

  it("rejects an input that contains an internal newline as a single-packet violation", () => {
    const record = analyzeJt1078(`${JT1078_0X9101}\n${JT1078_0X9102}`, "0x9101", "downstream");

    expect(record.kind).toBe("error");
    expect(record.error).toBe("仅支持单条报文，输入不能包含换行");
  });

  it("rejects empty and whitespace-only input without clearing the raw packet", () => {
    expect(analyzeJt1078("", "0x9101", "downstream").kind).toBe("error");
    expect(analyzeJt1078("   \t ", "0x9101", "downstream").kind).toBe("error");
  });
});