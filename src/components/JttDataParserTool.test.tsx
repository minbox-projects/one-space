import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { JttDataParserTool } from "@/components/JttDataParserTool";
import { renderWithProviders } from "@/test/mocks/render";
import {
  JT1078_0X9101,
  JT1078_UNLISTED_0X0200,
  JT808_BAD_CHECKSUM,
  JT808_F1_2013_0801_ESCAPED,
  JT808_POSITION_0200,
  JT808_POSITION_0704_TWO_FRAMES,
  JT809_2019_ENCRYPTED_1200,
  JT809_2019_UNENCRYPTED_0200,
} from "@/lib/jttDataParser/fixtures";

const F1_SERIALIZED =
  "帧结构:\n  起始标志: 0x7E\n  消息 ID: 0x0801\n  消息体属性:\n    消息体长度: 42\n    加密方式: 无\n    分包: 否\n    版本: 2013\n  终端手机号: 013123456789\n  消息流水号: 1024\n  校验和: 0x76\n  结束标志: 0x7E\n协议体 (0x0801 多媒体上传):\n  多媒体 ID: 1\n  多媒体类型: 0x02 (音频)\n  多媒体格式编码: 0x03 (MP3)\n  事件项编码: 0x04\n  通道 ID: 1\n  位置信息:\n    报警标志: 0x00000000\n    状态: 0x00000002\n    经度: 118798298\n    纬度: 32062838\n    海拔: 12\n    速度: 45\n    方向: 90\n    时间: 2026-09-04 14:30:00\n  多媒体数据 (Hex): 01027E7D0304";

const BATCH_SERIALIZED = F1_SERIALIZED;

const JT808_INPUT = /JT808 packet input|JT808 报文输入/;
const JT809_INPUT = /JT809 packet input|JT809 报文输入/;
const JT1078_INPUT = /JT1078 packet input|JT1078 报文输入/;
const HEX_INPUT = /Hex input|Hex 输入/;
const ANALYZE = /Analyze|解析/;
const CLEAR = /Clear|清空/;
const COPY = /Copy Result|复制结果/;
const RESULT = /Result|解析结果|转换结果/;
const HISTORY = /History|历史报文/;
const PREVIEW = (text: string) => `${text.slice(0, 30)}…`;

function mockClipboard(writeText: ReturnType<typeof vi.fn>) {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
}

async function analyzeJt808Packet(user: ReturnType<typeof userEvent.setup>, packet: string) {
  fireEvent.change(screen.getByLabelText(JT808_INPUT), { target: { value: packet } });
  await user.click(screen.getByRole("button", { name: ANALYZE }));
}

describe("JttDataParserTool", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("keeps each mounted tab's input, controls, result, and errors when switching tabs", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, JT808_F1_2013_0801_ESCAPED);
    expect(screen.getByText("消息 ID: 0x0801")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /JT809/ }));
    fireEvent.change(screen.getByLabelText(JT809_INPUT), {
      target: { value: "000000401234020000000001000000022609041430000000000027E88B8F413132333435000100000000000000020714B7DA01E93D76000C002D005A2609041430009A067B7E" },
    });
    await user.selectOptions(screen.getByLabelText(/Version|版本/), "2019");
    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getByText("车牌号: 苏A12345")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /JT808/ }));
    expect(screen.getByLabelText(JT808_INPUT)).toHaveValue(JT808_F1_2013_0801_ESCAPED);
    expect(screen.getByText("消息 ID: 0x0801")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /JT809/ }));
    expect(screen.getByLabelText(JT809_INPUT)).toHaveValue(
      "000000401234020000000001000000022609041430000000000027E88B8F413132333435000100000000000000020714B7DA01E93D76000C002D005A2609041430009A067B7E",
    );
    expect(screen.getByLabelText(/Version|版本/)).toHaveValue("2019");
    expect(screen.getByText("车牌号: 苏A12345")).toBeInTheDocument();
  });

  it("resets every tab to its initial state when the component unmounts and mounts again", async () => {
    const user = userEvent.setup();
    const { unmount } = renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, JT808_F1_2013_0801_ESCAPED);
    await user.click(screen.getByRole("tab", { name: /JT809/ }));
    fireEvent.change(screen.getByLabelText(JT809_INPUT), { target: { value: "ABCD" } });
    await user.selectOptions(screen.getByLabelText(/Encryption|加密/), "encrypted");
    fireEvent.change(screen.getByLabelText(/^M1$/), { target: { value: "42" } });
    await user.click(screen.getByRole("tab", { name: /Hex/ }));
    fireEvent.change(screen.getByLabelText(HEX_INPUT), { target: { value: "48656C6C6F" } });

    unmount();
    renderWithProviders(<JttDataParserTool />);

    expect(screen.getByLabelText(JT808_INPUT)).toHaveValue("");
    await user.click(screen.getByRole("tab", { name: /JT809/ }));
    expect(screen.getByLabelText(JT809_INPUT)).toHaveValue("");
    expect(screen.getByLabelText(/Encryption|加密/)).toHaveValue("unencrypted");
    expect(screen.queryByLabelText(/^M1$/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: /Hex/ }));
    expect(screen.getByLabelText(HEX_INPUT)).toHaveValue("");
  });

  it("offers the five public JT808 modes and never Ruiding or GPS51", () => {
    renderWithProviders(<JttDataParserTool />);

    const modeSelect = screen.getByLabelText(/Mode|模式/);
    expect(within(modeSelect).getByRole("option", { name: /Automatic|自动识别/ })).toBeInTheDocument();
    expect(within(modeSelect).getByRole("option", { name: /JT1078 Extension|JT1078 扩展/ })).toBeInTheDocument();
    expect(within(modeSelect).getByRole("option", { name: /Jiangsu Active Safety|江苏主动安全/ })).toBeInTheDocument();
    expect(within(modeSelect).getByRole("option", { name: /Guangdong Active Safety|广东主动安全/ })).toBeInTheDocument();
    expect(within(modeSelect).getByRole("option", { name: /Force 2013|强制 2013/ })).toBeInTheDocument();
    expect(within(modeSelect).queryByRole("option", { name: /Ruid|GPS51|锐明/ })).not.toBeInTheDocument();
  });

  it("clears the JT809 result and error when a top selector changes but keeps the raw input", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);
    await user.click(screen.getByRole("tab", { name: /JT809/ }));

    fireEvent.change(screen.getByLabelText(JT809_INPUT), {
      target: { value: JT809_2019_UNENCRYPTED_0200 },
    });
    await user.selectOptions(screen.getByLabelText(/Version|版本/), "2019");
    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getByText("车牌号: 苏A12345")).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText(/Encryption|加密/), "encrypted");
    expect(screen.queryByText("车牌号: 苏A12345")).not.toBeInTheDocument();
    expect(screen.getByLabelText(JT809_INPUT)).toHaveValue(JT809_2019_UNENCRYPTED_0200);
  });

  it("shows M1, IA1, and IC1 controls defaulting to 0 only in encrypted JT809 mode", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);
    await user.click(screen.getByRole("tab", { name: /JT809/ }));

    expect(screen.queryByLabelText(/^M1$/)).not.toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText(/Encryption|加密/), "encrypted");
    expect(screen.getByLabelText(/^M1$/)).toHaveValue("0");
    expect(screen.getByLabelText(/^IA1$/)).toHaveValue("0");
    expect(screen.getByLabelText(/^IC1$/)).toHaveValue("0");
  });

  it("renders encrypted JT809 frame fields and never claims decryption", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);
    await user.click(screen.getByRole("tab", { name: /JT809/ }));
    await user.selectOptions(screen.getByLabelText(/Encryption|加密/), "encrypted");

    fireEvent.change(screen.getByLabelText(JT809_INPUT), {
      target: { value: JT809_2019_ENCRYPTED_1200 },
    });
    await user.click(screen.getByRole("button", { name: ANALYZE }));

    expect(screen.getByText("报文类型: 0x1200")).toBeInTheDocument();
    expect(screen.getByText("加密标识: 0x02 (加密)")).toBeInTheDocument();
    expect(screen.getByText(/无可审计的公开解密规范/)).toBeInTheDocument();
    expect(screen.queryByText(/解密后/)).not.toBeInTheDocument();
  });

  it("keeps input, selectors, parameter values, and prior result on a JT809 parameter error", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);
    await user.click(screen.getByRole("tab", { name: /JT809/ }));
    await user.selectOptions(screen.getByLabelText(/Encryption|加密/), "encrypted");

    fireEvent.change(screen.getByLabelText(JT809_INPUT), {
      target: { value: JT809_2019_ENCRYPTED_1200 },
    });
    fireEvent.change(screen.getByLabelText(/^IA1$/), { target: { value: "999999999999" } });
    await user.click(screen.getByRole("button", { name: ANALYZE }));

    expect(screen.getByRole("alert")).toHaveTextContent(/IA1/);
    expect(screen.getByLabelText(JT809_INPUT)).toHaveValue(JT809_2019_ENCRYPTED_1200);
    expect(screen.getByLabelText(/^IA1$/)).toHaveValue("999999999999");

    fireEvent.change(screen.getByLabelText(/^IA1$/), { target: { value: "1" } });
    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getByText("报文类型: 0x1200")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("exposes the four JT1078 operations and both directions", async () => {
    renderWithProviders(<JttDataParserTool />);
    await userEvent.setup().click(screen.getByRole("tab", { name: /JT1078/ }));

    const operation = screen.getByLabelText(/Operation|操作/);
    for (const op of ["0x9101", "0x9102", "0x9205", "0x9206"]) {
      expect(within(operation).getByRole("option", { name: new RegExp(op) })).toBeInTheDocument();
    }
    const direction = screen.getByLabelText(/Direction|方向/);
    expect(within(direction).getByRole("option", { name: /Upstream|上行/ })).toBeInTheDocument();
    expect(within(direction).getByRole("option", { name: /Downstream|下行/ })).toBeInTheDocument();
  });

  it("parses a matching JT1078 direction and reports a specific mismatch error", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);
    await user.click(screen.getByRole("tab", { name: /JT1078/ }));

    fireEvent.change(screen.getByLabelText(JT1078_INPUT), {
      target: { value: JT1078_0X9101 },
    });
    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getByText("服务器地址: 192.168.1.100")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(JT1078_INPUT), {
      target: { value: JT1078_0X9101 },
    });
    await user.selectOptions(screen.getByLabelText(/Direction|方向/), "upstream");
    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getByRole("alert")).toHaveTextContent(/不匹配/);
    expect(screen.getByLabelText(JT1078_INPUT)).toHaveValue(JT1078_0X9101);
  });

  it("reports an unknown JT1078 body as unsupported with frame fields", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);
    await user.click(screen.getByRole("tab", { name: /JT1078/ }));

    fireEvent.change(screen.getByLabelText(JT1078_INPUT), {
      target: { value: JT1078_UNLISTED_0X0200 },
    });
    await user.click(screen.getByRole("button", { name: ANALYZE }));

    expect(screen.getByText("状态: 暂不支持该协议体")).toBeInTheDocument();
    expect(screen.getByText("消息 ID: 0x0200")).toBeInTheDocument();
    expect(screen.getByText("支持状态: 不在本模式冻结支持范围内")).toBeInTheDocument();
  });

  it("converts hex to UTF-8 while preserving LF line positions", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);
    await user.click(screen.getByRole("tab", { name: /Hex/ }));

    fireEvent.change(screen.getByLabelText(HEX_INPUT), {
      target: { value: "48 65 6C 6C 6F\n\nE4BDA0E5A5BD" },
    });
    await user.click(screen.getByRole("button", { name: /Convert|转换/ }));

    const result = screen.getByRole("region", { name: RESULT });
    expect(result).toHaveTextContent("Hello");
    expect(result).toHaveTextContent("你好");
  });

  it("converts UTF-8 to spaced uppercase hex with blank lines preserved", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);
    await user.click(screen.getByRole("tab", { name: /Hex/ }));
    await user.selectOptions(screen.getByLabelText(/Direction|方向/), "utf8-to-hex");

    fireEvent.change(screen.getByLabelText(HEX_INPUT), { target: { value: "Hello\n\n你好" } });
    await user.click(screen.getByRole("button", { name: /Convert|转换/ }));

    const result = screen.getByRole("region", { name: RESULT });
    expect(result.textContent).toBe("48 65 6C 6C 6F\n\nE4 BD A0 E5 A5 BD");
  });

  it("provides an example, clear, convert, and copy actions on the Hex tab", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    mockClipboard(writeText);
    renderWithProviders(<JttDataParserTool />);
    await user.click(screen.getByRole("tab", { name: /Hex/ }));

    await user.click(screen.getByRole("button", { name: /Example|示例/ }));
    const input = screen.getByLabelText(HEX_INPUT);
    expect(input).toHaveValue("48656C6C6F 20576F726C6421");

    await user.click(screen.getByRole("button", { name: /Convert|转换/ }));
    expect(screen.getByRole("region", { name: RESULT })).toHaveTextContent("Hello World!");

    await user.click(screen.getByRole("button", { name: COPY }));
    expect(writeText).toHaveBeenCalledWith("Hello World!");

    await user.click(screen.getByRole("button", { name: CLEAR }));
    expect(input).toHaveValue("");
    expect(screen.queryByRole("region", { name: RESULT })).not.toBeInTheDocument();
  });

  it("rejects multi-line JT809 and JT1078 input as single packets", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    await user.click(screen.getByRole("tab", { name: /JT809/ }));
    fireEvent.change(screen.getByLabelText(JT809_INPUT), {
      target: { value: `${JT809_2019_UNENCRYPTED_0200}\n${JT809_2019_UNENCRYPTED_0200}` },
    });
    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getByRole("alert")).toHaveTextContent(/单条报文/);

    await user.click(screen.getByRole("tab", { name: /JT1078/ }));
    fireEvent.change(screen.getByLabelText(JT1078_INPUT), {
      target: { value: `${JT1078_0X9101}\n${JT1078_0X9101}` },
    });
    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getByRole("alert")).toHaveTextContent(/单条报文/);
  });

  it("retains raw input and reports a specific error for an invalid JT808 frame", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, JT808_BAD_CHECKSUM);
    expect(screen.getByRole("alert")).toHaveTextContent(/校验和不匹配/);
    expect(screen.getByLabelText(JT808_INPUT)).toHaveValue(JT808_BAD_CHECKSUM);
  });

  it("retains raw input and reports a specific error for odd-length hex", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);
    await user.click(screen.getByRole("tab", { name: /Hex/ }));

    fireEvent.change(screen.getByLabelText(HEX_INPUT), { target: { value: "4 8 6" } });
    await user.click(screen.getByRole("button", { name: /Convert|转换/ }));

    expect(screen.getByRole("alert")).toHaveTextContent(/奇数/);
    expect(screen.getByLabelText(HEX_INPUT)).toHaveValue("4 8 6");
  });

  it("renders the stable Chinese tree with line context and ordered batch records", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, `${JT808_F1_2013_0801_ESCAPED}\n\n${JT808_BAD_CHECKSUM}\n`);
    const region = screen.getByRole("region", { name: RESULT });

    expect(within(region).getByText("第 1 行")).toBeInTheDocument();
    expect(within(region).getByText("消息 ID: 0x0801")).toBeInTheDocument();
    expect(within(region).getByText("多媒体类型: 0x02 (音频)")).toBeInTheDocument();
    expect(within(region).getByText("第 3 行")).toBeInTheDocument();
    expect(within(region).getByText("说明: 校验和不匹配")).toBeInTheDocument();
  });

  it("renders a 0x0200 position report as the reference JSON and copies it verbatim", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    mockClipboard(writeText);
    renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, JT808_POSITION_0200);
    const region = screen.getByRole("region", { name: RESULT });
    expect(within(region).getByText(/第 1 行/)).toBeInTheDocument();
    expect(within(region).getByText(/状态: 成功/)).toBeInTheDocument();
    expect(within(region).getByText(/"\[7E\]开始": 126/)).toBeInTheDocument();
    expect(within(region).getByText(/"\[018920259024\]终端手机号": "018920259024"/)).toBeInTheDocument();
    expect(within(region).getByText(/"\[0001BBF0\]里程": 113648/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: COPY }));
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText.mock.calls[0][0]).toContain('"附加信息列表"');
    expect(writeText.mock.calls[0][0]).toContain('"[15]校验码": 21');
  });

  it("copies the exact serialized semantic result for one record and for a batch", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    mockClipboard(writeText);
    renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, JT808_F1_2013_0801_ESCAPED);
    await user.click(screen.getByRole("button", { name: COPY }));
    expect(writeText).toHaveBeenNthCalledWith(1, F1_SERIALIZED);

    await analyzeJt808Packet(user, `${JT808_F1_2013_0801_ESCAPED}\n${JT808_BAD_CHECKSUM}`);
    await user.click(screen.getByRole("button", { name: COPY }));
    expect(writeText).toHaveBeenNthCalledWith(2, BATCH_SERIALIZED);
  });

  it("collapses and expands each result block by default expanded on header click", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, JT808_POSITION_0704_TWO_FRAMES);
    const region = screen.getByRole("region", { name: RESULT });
    expect(within(region).getAllByText(/"\[7E\]开始": 126/)).toHaveLength(2);

    const headers = within(region).getAllByRole("button", { name: /第 1 行/ });
    await user.click(headers[0]);
    expect(within(region).getAllByText(/"\[7E\]开始": 126/)).toHaveLength(1);

    await user.click(headers[0]);
    expect(within(region).getAllByText(/"\[7E\]开始": 126/)).toHaveLength(2);
  });

  it("detects consecutive multi-frame input and offers an auto line-break action", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    fireEvent.change(screen.getByLabelText(JT808_INPUT), {
      target: { value: JT808_POSITION_0704_TWO_FRAMES },
    });
    const wrapButton = screen.getByRole("button", { name: /自动换行|Auto line-break/ });
    expect(wrapButton).toBeInTheDocument();

    await user.click(wrapButton);
    const input = screen.getByLabelText(JT808_INPUT) as HTMLTextAreaElement;
    expect(input.value.split("\n")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: /自动换行|Auto line-break/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getAllByText(/"\[7E\]开始": 126/)).toHaveLength(2);
  });

  it("only mutates the active tab when analyzing or clearing", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, JT808_F1_2013_0801_ESCAPED);
    await user.click(screen.getByRole("button", { name: CLEAR }));
    expect(screen.getByLabelText(JT808_INPUT)).toHaveValue("");

    await user.click(screen.getByRole("tab", { name: /JT809/ }));
    fireEvent.change(screen.getByLabelText(JT809_INPUT), {
      target: { value: JT809_2019_UNENCRYPTED_0200 },
    });
    await user.selectOptions(screen.getByLabelText(/Version|版本/), "2019");
    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getByText("车牌号: 苏A12345")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /JT808/ }));
    expect(screen.getByLabelText(JT808_INPUT)).toHaveValue("");
    expect(screen.queryByText("车牌号: 苏A12345")).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: /JT809/ }));
    expect(screen.getByText("车牌号: 苏A12345")).toBeInTheDocument();
  });

  it("reports recoverable copy feedback for success, no result, and clipboard denial", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    mockClipboard(writeText);
    renderWithProviders(<JttDataParserTool />);

    await user.click(screen.getByRole("button", { name: COPY }));
    expect(screen.getByText(/Nothing to copy|没有可复制的结果/)).toBeInTheDocument();
    expect(writeText).not.toHaveBeenCalled();

    await analyzeJt808Packet(user, JT808_F1_2013_0801_ESCAPED);
    await user.click(screen.getByRole("button", { name: COPY }));
    expect(screen.getByText(/Result copied|结果已复制/)).toBeInTheDocument();

    writeText.mockRejectedValue(new Error("denied"));
    await user.click(screen.getByRole("button", { name: COPY }));
    expect(screen.getByText(/Unable to copy result|无法复制结果/)).toBeInTheDocument();
    expect(screen.getByLabelText(JT808_INPUT)).toHaveValue(JT808_F1_2013_0801_ESCAPED);
  });

  it("allows editing, switching tabs, and re-analyzing after a completed analysis", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, JT808_F1_2013_0801_ESCAPED);
    expect(screen.getByText("消息 ID: 0x0801")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(JT808_INPUT), {
      target: { value: `${JT808_F1_2013_0801_ESCAPED}\n${JT808_F1_2013_0801_ESCAPED}` },
    });
    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getAllByText("消息 ID: 0x0801")).toHaveLength(2);

    await user.click(screen.getByRole("tab", { name: /Hex/ }));
    await user.click(screen.getByRole("tab", { name: /JT808/ }));
    expect(screen.getAllByText("消息 ID: 0x0801")).toHaveLength(2);
  });

  it("opens the history dialog and selecting an entry closes it and replaces the input entirely", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, JT808_F1_2013_0801_ESCAPED);
    await user.click(screen.getByRole("button", { name: HISTORY }));

    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText(PREVIEW(JT808_F1_2013_0801_ESCAPED))).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(JT808_INPUT), { target: { value: "7E00000000" } });
    expect(screen.getByLabelText(JT808_INPUT)).toHaveValue("7E00000000");

    await user.click(within(dialog).getByRole("button", { name: /Select|选择/ }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByLabelText(JT808_INPUT)).toHaveValue(JT808_F1_2013_0801_ESCAPED);
  });

  it("persists recent history across unmount and remount without restoring the input", async () => {
    const user = userEvent.setup();
    const { unmount } = renderWithProviders(<JttDataParserTool />);
    await analyzeJt808Packet(user, JT808_F1_2013_0801_ESCAPED);
    unmount();

    renderWithProviders(<JttDataParserTool />);
    expect(screen.getByLabelText(JT808_INPUT)).toHaveValue("");
    await user.click(screen.getByRole("button", { name: HISTORY }));
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText(PREVIEW(JT808_F1_2013_0801_ESCAPED))).toBeInTheDocument();
  });

  it("lists history per tab, keeps failed parse input, and shows an empty state without records", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    await user.click(screen.getByRole("button", { name: HISTORY }));
    const emptyDialog = screen.getByRole("dialog");
    expect(within(emptyDialog).getByText(/No history messages yet|暂无历史报文/)).toBeInTheDocument();
    await user.keyboard("{Escape}");

    await analyzeJt808Packet(user, JT808_F1_2013_0801_ESCAPED);
    await user.click(screen.getByRole("tab", { name: /JT809/ }));
    fireEvent.change(screen.getByLabelText(JT809_INPUT), {
      target: { value: `${JT809_2019_UNENCRYPTED_0200}\n${JT809_2019_UNENCRYPTED_0200}` },
    });
    await user.click(screen.getByRole("button", { name: ANALYZE }));
    expect(screen.getByRole("alert")).toHaveTextContent(/单条报文/);

    const failedInput = `${JT809_2019_UNENCRYPTED_0200}\n${JT809_2019_UNENCRYPTED_0200}`;
    await user.click(screen.getByRole("button", { name: HISTORY }));
    const jt809Dialog = screen.getByRole("dialog");
    expect(within(jt809Dialog).getByText(PREVIEW(failedInput))).toBeInTheDocument();
    expect(within(jt809Dialog).queryByText(PREVIEW(JT808_F1_2013_0801_ESCAPED))).not.toBeInTheDocument();
    await user.keyboard("{Escape}");

    await user.click(screen.getByRole("tab", { name: /JT808/ }));
    await user.click(screen.getByRole("button", { name: HISTORY }));
    const jt808Dialog = screen.getByRole("dialog");
    expect(within(jt808Dialog).getByText(PREVIEW(JT808_F1_2013_0801_ESCAPED))).toBeInTheDocument();
    expect(within(jt808Dialog).queryByText(PREVIEW(failedInput))).not.toBeInTheDocument();
  });

  it("shows the full message in a nested dialog through the view-all link", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JttDataParserTool />);

    await analyzeJt808Packet(user, JT808_F1_2013_0801_ESCAPED);
    await user.click(screen.getByRole("button", { name: HISTORY }));
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("button", { name: /View All|查看全部/ }));
    const fullDialog = screen.getByRole("dialog");
    expect(within(fullDialog).getByText(JT808_F1_2013_0801_ESCAPED)).toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(within(screen.getByRole("dialog")).queryByText(JT808_F1_2013_0801_ESCAPED)).not.toBeInTheDocument();
  });
});