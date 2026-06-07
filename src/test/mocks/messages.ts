import { vi } from "vitest";

export const safeRecordMessageMock = vi.fn(async () => undefined);
export const recordMessageMock = vi.fn(async () => undefined);
export const getUnreadMessageCountMock = vi.fn(async () => 0);

vi.mock("@/lib/messages", async () => {
  const actual = await vi.importActual<typeof import("@/lib/messages")>(
    "@/lib/messages",
  );
  return {
    ...actual,
    safeRecordMessage: safeRecordMessageMock,
    recordMessage: recordMessageMock,
    getUnreadMessageCount: getUnreadMessageCountMock,
  };
});

export function resetMessageMocks() {
  safeRecordMessageMock.mockReset();
  safeRecordMessageMock.mockResolvedValue(undefined);
  recordMessageMock.mockReset();
  recordMessageMock.mockResolvedValue(undefined);
  getUnreadMessageCountMock.mockReset();
  getUnreadMessageCountMock.mockResolvedValue(0);
}
