import { vi } from "vitest";

export const shortLinkConfigStatusMock = vi.fn();
export const shortLinkSaveTokenMock = vi.fn();
export const shortLinkDeleteTokenMock = vi.fn();
export const shortLinkCreateMock = vi.fn();

// `ble-lock` command fixtures. Each mock receives exactly the arguments the
// thin `src/lib/bleLock.ts` wrappers forward to `invoke`, so tests can assert
// the wire contract independently of the tool component.
export const bleLockGetStatusMock = vi.fn();
export const bleLockSaveConfigMock = vi.fn();
export const bleLockScanStartMock = vi.fn();
export const bleLockScanStopMock = vi.fn();
export const bleLockSelectDeviceMock = vi.fn();
export const bleLockClearDeviceMock = vi.fn();
export const bleLockSetPasswordMock = vi.fn();
export const bleLockDeletePasswordMock = vi.fn();
export const bleLockRequestAccessibilityMock = vi.fn();
export const bleLockLockNowMock = vi.fn();

function invokeCommandMock(command: string, args?: Record<string, unknown>) {
  switch (command) {
    case "short_link_config_status":
      return shortLinkConfigStatusMock();
    case "short_link_save_token":
      return shortLinkSaveTokenMock(args?.token);
    case "short_link_delete_token":
      return shortLinkDeleteTokenMock();
    case "short_link_create":
      return shortLinkCreateMock(args?.url);
    case "ble_lock_get_status":
      return bleLockGetStatusMock();
    case "ble_lock_save_config":
      return bleLockSaveConfigMock(args?.config);
    case "ble_lock_scan_start":
      return bleLockScanStartMock();
    case "ble_lock_scan_stop":
      return bleLockScanStopMock();
    case "ble_lock_select_device":
      return bleLockSelectDeviceMock(args?.deviceId, args?.deviceName);
    case "ble_lock_clear_device":
      return bleLockClearDeviceMock();
    case "ble_lock_set_password":
      return bleLockSetPasswordMock(args?.password);
    case "ble_lock_delete_password":
      return bleLockDeletePasswordMock();
    case "ble_lock_request_accessibility":
      return bleLockRequestAccessibilityMock();
    case "ble_lock_lock_now":
      return bleLockLockNowMock();
    default:
      return Promise.resolve(undefined);
  }
}

export const invokeMock = vi.fn(invokeCommandMock);
type ListenSignature = (
  eventName: string,
  handler: (event: { payload?: unknown }) => unknown,
) => Promise<() => void>;
export const listenMock = vi.fn<ListenSignature>(async () => vi.fn());
export const emitMock = vi.fn(async () => undefined);
export const dialogOpenMock = vi.fn();
export const dialogSaveMock = vi.fn();
export const startDraggingMock = vi.fn(async () => undefined);
export const isVisibleMock = vi.fn(async () => true);
export const getCurrentWindowMock = vi.fn(() => ({
  startDragging: startDraggingMock,
  isVisible: isVisibleMock,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
  emit: emitMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: dialogOpenMock,
  save: dialogSaveMock,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: getCurrentWindowMock,
}));

export function resetTauriMocks() {
  invokeMock.mockReset();
  shortLinkConfigStatusMock.mockReset();
  shortLinkConfigStatusMock.mockResolvedValue({ configured: false });
  shortLinkSaveTokenMock.mockReset();
  shortLinkSaveTokenMock.mockResolvedValue({ configured: true });
  shortLinkDeleteTokenMock.mockReset();
  shortLinkDeleteTokenMock.mockResolvedValue({ configured: false });
  shortLinkCreateMock.mockReset();
  bleLockGetStatusMock.mockReset();
  bleLockSaveConfigMock.mockReset();
  bleLockScanStartMock.mockReset();
  bleLockScanStopMock.mockReset();
  bleLockSelectDeviceMock.mockReset();
  bleLockClearDeviceMock.mockReset();
  bleLockSetPasswordMock.mockReset();
  bleLockDeletePasswordMock.mockReset();
  bleLockRequestAccessibilityMock.mockReset();
  bleLockLockNowMock.mockReset();
  invokeMock.mockImplementation(invokeCommandMock);
  listenMock.mockReset();
  listenMock.mockImplementation(async () => vi.fn());
  emitMock.mockReset();
  emitMock.mockResolvedValue(undefined);
  dialogOpenMock.mockReset();
  dialogSaveMock.mockReset();
  startDraggingMock.mockReset();
  startDraggingMock.mockResolvedValue(undefined);
  isVisibleMock.mockReset();
  isVisibleMock.mockResolvedValue(true);
  getCurrentWindowMock.mockReset();
  getCurrentWindowMock.mockReturnValue({
    startDragging: startDraggingMock,
    isVisible: isVisibleMock,
  });
}
