import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { invokeMock } from "@/test/mocks/tauri";
import {
  JT1078_0X9101,
  JT808_F1_2013_0801_ESCAPED,
  JT808_F2_2011_0801,
  JT808_F3_FRAGMENT_1,
  JT808_F3_FRAGMENT_2,
  JT808_F3_FRAGMENT_3,
  JT809_2019_ENCRYPTED_1200,
  JT809_2019_UNENCRYPTED_0200,
} from "./fixtures";
import {
  analyzeJt1078,
  analyzeJt808,
  analyzeJt809,
  convertHexLines,
} from "./index";

describe("offline parser isolation", () => {
  const fetchSpy = vi.fn();
  const xhrSpy = vi.fn();
  const setItemSpy = vi.spyOn(Storage.prototype, "setItem");

  beforeAll(() => {
    vi.stubGlobal("fetch", fetchSpy);
    vi.stubGlobal(
      "XMLHttpRequest",
      class XmlHttpRequestDenied {
        constructor() {
          xhrSpy();
        }
      },
    );
  });

  afterAll(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("completes every frozen fixture synchronously with no network, storage, or Tauri call", () => {
    const outcomes = [
      analyzeJt808(JT808_F1_2013_0801_ESCAPED, "automatic"),
      analyzeJt808(JT808_F2_2011_0801, "force-2013"),
      analyzeJt808([JT808_F3_FRAGMENT_1, JT808_F3_FRAGMENT_2, JT808_F3_FRAGMENT_3].join("\n"), "automatic"),
      analyzeJt809(JT809_2019_UNENCRYPTED_0200, "2019", "unencrypted", { m1: "1", ia1: "2", ic1: "3" }),
      analyzeJt809(JT809_2019_ENCRYPTED_1200, "2019", "encrypted", { m1: "1", ia1: "2", ic1: "3" }),
      analyzeJt1078(JT1078_0X9101, "0x9101", "downstream"),
      convertHexLines("48656C6C6F\n\nE4BDA0E5A5BD", "hex-to-utf8"),
      convertHexLines("Hello\n\n你好", "utf8-to-hex"),
    ];

    for (const outcome of outcomes) {
      expect(outcome).not.toBeInstanceOf(Promise);
    }

    expect(fetchSpy).not.toHaveBeenCalled();
    expect(xhrSpy).not.toHaveBeenCalled();
    expect(setItemSpy).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("can be run again after completion with unchanged, fresh results", () => {
    const first = analyzeJt808(JT808_F1_2013_0801_ESCAPED, "automatic");
    const second = analyzeJt808(JT808_F1_2013_0801_ESCAPED, "automatic");

    expect(first).toEqual(second);
    expect(first[0].kind).toBe("success");
  });
});