import { vi } from "vitest";

export const invokeMock = vi.fn();
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
