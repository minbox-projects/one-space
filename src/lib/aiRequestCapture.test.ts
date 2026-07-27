import { describe, expect, it, vi } from "vitest";
import {
  aiRequestCaptureClear,
  aiRequestCaptureExportHar,
  aiRequestCaptureGenerateCurl,
  aiRequestCaptureGet,
  aiRequestCaptureGetConfig,
  aiRequestCaptureList,
  aiRequestCaptureSaveConfig,
  aiRequestCaptureStart,
  aiRequestCaptureStatus,
  aiRequestCaptureStop,
  subscribeAiRequestCaptureStatus,
  subscribeAiRequestCaptureUpdates,
} from "@/lib/aiRequestCapture";
import type { CaptureListQuery } from "@/lib/aiRequestCapture";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

describe("aiRequestCapture IPC", () => {
  it("calls each capture command with the Rust contract arguments", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    const config = { enabled: true, port: 17688, upstreamBaseUrl: "https://api.example.com" };
    const query: CaptureListQuery = { search: "chat", states: ["completed"], page: 2, pageSize: 20 };

    await aiRequestCaptureGetConfig();
    await aiRequestCaptureSaveConfig(config);
    await aiRequestCaptureStart();
    await aiRequestCaptureStop();
    await aiRequestCaptureStatus();
    await aiRequestCaptureList(query);
    await aiRequestCaptureGet("capture-1");
    await aiRequestCaptureClear();
    await aiRequestCaptureExportHar({ query, outputPath: "/tmp/captures.har" });
    await aiRequestCaptureGenerateCurl("capture-1");

    expect(invokeMock.mock.calls).toEqual([
      ["ai_request_capture_get_config"],
      ["ai_request_capture_save_config", { config }],
      ["ai_request_capture_start"],
      ["ai_request_capture_stop"],
      ["ai_request_capture_status"],
      ["ai_request_capture_list", { query }],
      ["ai_request_capture_get", { id: "capture-1" }],
      ["ai_request_capture_clear"],
      ["ai_request_capture_export_har", { input: { query, outputPath: "/tmp/captures.har" } }],
      ["ai_request_capture_generate_curl", { id: "capture-1" }],
    ]);
  });

  it("normalizes command failures into a capture error", async () => {
    resetTauriMocks();
    invokeMock.mockRejectedValue("runtime unavailable");

    await expect(aiRequestCaptureStatus()).rejects.toMatchObject({
      name: "AiRequestCaptureError",
      message: "runtime unavailable",
    });
  });

  it("subscribes to both capture events and returns their teardown functions", async () => {
    resetTauriMocks();
    const updated = vi.fn();
    const status = vi.fn();
    const removeUpdated = vi.fn();
    const removeStatus = vi.fn();
    listenMock.mockResolvedValueOnce(removeUpdated).mockResolvedValueOnce(removeStatus);

    await expect(subscribeAiRequestCaptureUpdates(updated)).resolves.toBe(removeUpdated);
    await expect(subscribeAiRequestCaptureStatus(status)).resolves.toBe(removeStatus);

    expect(listenMock).toHaveBeenNthCalledWith(1, "ai-request-capture-updated", expect.any(Function));
    expect(listenMock).toHaveBeenNthCalledWith(2, "ai-request-capture-status-update", expect.any(Function));
  });
});
