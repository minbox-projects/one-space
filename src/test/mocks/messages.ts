import { vi } from "vitest";

export const safeRecordMessageMock = vi.fn(async (_input: any) => undefined);
export const recordMessageMock = vi.fn(async (_input: any) => undefined);
export const recordAppMessageMock = vi.fn(async (_input: any) => null);
export const getUnreadMessageCountMock = vi.fn(async () => 0);

export const notifyMessageMock = {
  error: vi.fn(async (input: any) => recordAppMessageMock({ ...input, severity: "error" })),
  warn: vi.fn(async (input: any) => recordAppMessageMock({ ...input, severity: "warning" })),
  info: vi.fn(async (input: any) => recordAppMessageMock({ ...input, severity: "info" })),
  success: vi.fn(async (input: any) => recordAppMessageMock({ ...input, severity: "success" })),
};

vi.mock("@/lib/messages", async () => {
  const actual = await vi.importActual<typeof import("@/lib/messages")>(
    "@/lib/messages",
  );
  return {
    ...actual,
    safeRecordMessage: safeRecordMessageMock,
    recordMessage: recordMessageMock,
    recordAppMessage: recordAppMessageMock,
    notifyMessage: notifyMessageMock,
    getUnreadMessageCount: getUnreadMessageCountMock,
  };
});

export function resetMessageMocks() {
  safeRecordMessageMock.mockReset();
  safeRecordMessageMock.mockResolvedValue(undefined);
  recordMessageMock.mockReset();
  recordMessageMock.mockResolvedValue(undefined);
  recordAppMessageMock.mockReset();
  recordAppMessageMock.mockImplementation(async (input: any) => {
    const normalized = {
      ...input,
      dedupe_key: input.dedupe_key || input.dedupeKey,
    };
    await safeRecordMessageMock(normalized);
    return null;
  });
  notifyMessageMock.error.mockReset();
  notifyMessageMock.error.mockImplementation(async (input: any) =>
    recordAppMessageMock({ ...input, severity: "error" }),
  );
  notifyMessageMock.warn.mockReset();
  notifyMessageMock.warn.mockImplementation(async (input: any) =>
    recordAppMessageMock({ ...input, severity: "warning" }),
  );
  notifyMessageMock.info.mockReset();
  notifyMessageMock.info.mockImplementation(async (input: any) =>
    recordAppMessageMock({ ...input, severity: "info" }),
  );
  notifyMessageMock.success.mockReset();
  notifyMessageMock.success.mockImplementation(async (input: any) =>
    recordAppMessageMock({ ...input, severity: "success" }),
  );
  getUnreadMessageCountMock.mockReset();
  getUnreadMessageCountMock.mockResolvedValue(0);
}

resetMessageMocks();
