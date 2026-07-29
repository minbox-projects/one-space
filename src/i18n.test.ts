import { afterEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";

const AI_REQUEST_CAPTURE_KEYS = [
  "aiRequestCapture",
  "aiRequestCaptureLauncherDesc",
  "aiRequestCaptureDesc",
  "aiRequestCaptureEnabled",
  "aiRequestCaptureStatus",
  "aiRequestCaptureStatusRunning",
  "aiRequestCaptureStatusStopped",
  "aiRequestCapturePlaintextWarning",
  "aiRequestCapturePort",
  "aiRequestCaptureUpstreamBaseUrl",
  "aiRequestCaptureSaveApply",
  "aiRequestCaptureLocalBaseUrl",
  "aiRequestCaptureLastError",
  "aiRequestCaptureRequests",
  "aiRequestCaptureSearch",
  "aiRequestCaptureMethod",
  "aiRequestCaptureNoRequests",
  "aiRequestCaptureSelectRequest",
  "aiRequestCaptureOverview",
  "aiRequestCaptureRequest",
  "aiRequestCaptureResponse",
  "aiRequestCaptureHeaders",
  "aiRequestCaptureBody",
  "aiRequestCaptureText",
  "aiRequestCaptureTruncated",
  "aiRequestCaptureCopyOriginal",
  "aiRequestCaptureCopyStatus",
  "aiRequestCaptureCopyLocalBaseUrl",
  "aiRequestCaptureCopyUpstreamBaseUrl",
  "aiRequestCaptureCopyCurl",
  "aiRequestCaptureCopyProvider",
  "aiRequestCaptureCopyModel",
  "aiRequestCaptureCopyInputTokens",
  "aiRequestCaptureCopyOutputTokens",
  "aiRequestCaptureCopyTokens",
  "aiRequestCaptureCopyDuration",
  "aiRequestCaptureInputTokens",
  "aiRequestCaptureOutputTokens",
  "aiRequestCaptureDuration",
  "aiRequestCaptureTransferError",
  "aiRequestCapturePreviousPage",
  "aiRequestCaptureNextPage",
  "aiRequestCaptureRefreshed",
  "aiRequestCaptureExportHar",
  "aiRequestCaptureExportConfirm",
  "aiRequestCaptureExportConfirmTitle",
  "aiRequestCaptureExportConfirmAction",
  "aiRequestCaptureExported",
  "aiRequestCaptureClearHistory",
  "aiRequestCaptureClearConfirm",
  "aiRequestCaptureClearConfirmTitle",
  "aiRequestCaptureClearConfirmAction",
  "aiRequestCaptureCleared",
  "aiRequestCaptureIncompleteCurlConfirm",
  "aiRequestCaptureIncompleteCurlTitle",
  "aiRequestCaptureCopyAnyway",
  "aiRequestCaptureState_in_progress",
  "aiRequestCaptureState_completed",
  "aiRequestCaptureState_rejected",
  "aiRequestCaptureState_upstream_error",
  "aiRequestCaptureState_request_transfer_error",
  "aiRequestCaptureState_response_transfer_error",
  "aiRequestCaptureState_client_disconnected",
  "aiRequestCaptureState_interrupted",
] as const;

const FILE_SHARING_KEYS = [
  "fileSharing",
  "fileSharingDesc",
  "fileSharingLauncherDesc",
  "fileSharingChooseFiles",
  "fileSharingNetwork",
  "fileSharingStart",
  "fileSharingStop",
  "fileSharingWarning",
  "fileSharingState_completed",
] as const;

describe("AI 请求抓包国际化", () => {
  afterEach(async () => {
    await i18n.changeLanguage("zh");
  });

  it.each(["en", "zh"] as const)("为 %s 提供完整界面文案", async (language) => {
    await i18n.changeLanguage(language);

    for (const key of AI_REQUEST_CAPTURE_KEYS) {
      expect(i18n.t(key)).not.toBe(key);
    }
  });
});

describe("文件共享国际化", () => {
  it.each(["en", "zh"] as const)("为 %s 提供关键界面文案", async (language) => {
    await i18n.changeLanguage(language);
    for (const key of FILE_SHARING_KEYS) expect(i18n.t(key)).not.toBe(key);
  });
});
