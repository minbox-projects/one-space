import { beforeEach, describe, expect, it } from "vitest";
import {
  SHORT_LINK_ERROR_CODES,
  ShortLinkError,
  shortLinkConfigStatus,
  shortLinkCreate,
  shortLinkDeleteToken,
  shortLinkSaveToken,
} from "@/lib/shortLink";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

describe("short link IPC", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("使用固定命令名、参数与最小响应", async () => {
    invokeMock
      .mockResolvedValueOnce({ configured: true, token: "不得返回的旧 Token" })
      .mockResolvedValueOnce({ configured: true, token: "不得返回的旧 Token" })
      .mockResolvedValueOnce({ configured: false, token: "不得返回的旧 Token" })
      .mockResolvedValueOnce({
        longUrl: "https://example.com/long",
        shortUrl: "https://tinyurl.com/test-only",
        ignored: "extra",
      });

    await expect(shortLinkConfigStatus()).resolves.toEqual({ configured: true });
    await expect(shortLinkSaveToken("test-token-placeholder")).resolves.toEqual({ configured: true });
    await expect(shortLinkDeleteToken()).resolves.toEqual({ configured: false });
    await expect(shortLinkCreate("https://example.com/long")).resolves.toEqual({
      longUrl: "https://example.com/long",
      shortUrl: "https://tinyurl.com/test-only",
    });

    expect(invokeMock.mock.calls).toEqual([
      ["short_link_config_status"],
      ["short_link_save_token", { token: "test-token-placeholder" }],
      ["short_link_delete_token"],
      ["short_link_create", { url: "https://example.com/long" }],
    ]);
  });

  it.each(SHORT_LINK_ERROR_CODES)("按稳定 code 传递 %s 错误", async (code) => {
    invokeMock.mockRejectedValue({ code, message: "safe diagnostic" });

    await expect(shortLinkCreate("https://example.com")).rejects.toMatchObject({
      name: "ShortLinkError",
      code,
      diagnostic: "safe diagnostic",
      message: "safe diagnostic",
    });
  });

  it("不把非结构化错误或未知后端字段暴露为诊断信息", async () => {
    invokeMock.mockRejectedValue({ code: "unexpected", message: "sensitive backend detail" });

    await expect(shortLinkConfigStatus()).rejects.toEqual(new ShortLinkError("unknown"));
  });
});
