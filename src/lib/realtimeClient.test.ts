import { describe, expect, it } from "vitest";
import { formatTime, parseSseBlock, tryParseJson } from "./realtimeClient";

describe("realtimeClient utilities", () => {
  it("formats timestamps into HH:mm:ss.SSS accurately", () => {
    const ts = new Date("2026-11-06T10:30:15.123").getTime();
    expect(formatTime(ts)).toBe("10:30:15.123");
  });

  it("parses single standard SSE block", () => {
    const block = "event: message\ndata: {\"text\": \"hello world\"}\nid: 1";
    const parsed = parseSseBlock(block);

    expect(parsed).not.toBeNull();
    expect(parsed?.event).toBe("message");
    expect(parsed?.data).toBe("{\"text\": \"hello world\"}");
    expect(parsed?.id).toBe("1");
  });

  it("parses multiline SSE data block", () => {
    const block = "data: line 1\ndata: line 2\ndata: line 3";
    const parsed = parseSseBlock(block);

    expect(parsed).not.toBeNull();
    expect(parsed?.data).toBe("line 1\nline 2\nline 3");
  });

  it("ignores SSE comment lines", () => {
    const block = ": this is a comment\ndata: actual data";
    const parsed = parseSseBlock(block);

    expect(parsed).not.toBeNull();
    expect(parsed?.data).toBe("actual data");
  });

  it("returns null on empty or invalid SSE block", () => {
    expect(parseSseBlock("")).toBeNull();
    expect(parseSseBlock("   \n\n  ")).toBeNull();
  });

  it("correctly identifies and formats JSON payload", () => {
    const validJson = "{\"status\": \"ok\", \"count\": 42}";
    const res = tryParseJson(validJson);
    expect(res.isJson).toBe(true);
    expect(res.formatted).toContain("  \"status\": \"ok\"");

    const invalid = "plain text message";
    const res2 = tryParseJson(invalid);
    expect(res2.isJson).toBe(false);
    expect(res2.formatted).toBe("plain text message");
  });
});
