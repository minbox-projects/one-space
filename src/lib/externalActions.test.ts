import { beforeEach, describe, expect, it } from "vitest";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

import "@/test/mocks/tauri";

describe("externalActions", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("detects likely local paths", async () => {
    const mod = await import("@/lib/externalActions");
    expect(mod.isLikelyLocalPath("/tmp/demo")).toBe(true);
    expect(mod.isLikelyLocalPath("~/demo")).toBe(true);
    expect(mod.isLikelyLocalPath("C:\\demo")).toBe(true);
    expect(mod.isLikelyLocalPath("https://example.com")).toBe(false);
  });

  it("invokes tauri command for external urls", async () => {
    const mod = await import("@/lib/externalActions");
    await mod.openExternalUrl("https://example.com");
    expect(invokeMock).toHaveBeenCalledWith("open_external_url", {
      url: "https://example.com",
    });
  });

  it("invokes tauri command for local paths", async () => {
    const mod = await import("@/lib/externalActions");
    await mod.openLocalPath("/tmp/demo");
    expect(invokeMock).toHaveBeenCalledWith("open_local_path", {
      path: "/tmp/demo",
    });
  });
});
