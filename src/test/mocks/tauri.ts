import { vi } from "vitest";

export const shortLinkConfigStatusMock = vi.fn();
export const shortLinkSaveTokenMock = vi.fn();
export const shortLinkDeleteTokenMock = vi.fn();
export const shortLinkCreateMock = vi.fn();

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
    default:
      return Promise.resolve(undefined);
  }
}

export const invokeMock = vi.fn(invokeCommandMock);
export const listenMock = vi.fn(async () => vi.fn());
export const emitMock = vi.fn(async () => undefined);
export const dialogOpenMock = vi.fn();
export const dialogSaveMock = vi.fn();
export const startDraggingMock = vi.fn(async () => undefined);
export const getCurrentWindowMock = vi.fn(() => ({
  startDragging: startDraggingMock,
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
  invokeMock.mockImplementation(invokeCommandMock);
  listenMock.mockReset();
  listenMock.mockImplementation(async () => vi.fn());
  emitMock.mockReset();
  emitMock.mockResolvedValue(undefined);
  dialogOpenMock.mockReset();
  dialogSaveMock.mockReset();
  startDraggingMock.mockReset();
  startDraggingMock.mockResolvedValue(undefined);
  getCurrentWindowMock.mockReset();
  getCurrentWindowMock.mockReturnValue({
    startDragging: startDraggingMock,
  });
}
