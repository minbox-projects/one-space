import type { TFunction } from "i18next";
import { vi } from "vitest";

export function createMockActionContext() {
  return {
    t: (((_key: string, fallback?: string, options?: Record<string, unknown>) => {
      if (!fallback) {
        return _key;
      }
      if (!options) {
        return fallback;
      }
      return Object.entries(options).reduce((text, [key, value]) => {
        return text.replace(`{{${key}}}`, String(value));
      }, fallback);
    }) as unknown) as TFunction,
    confirm: vi.fn(async () => true),
    pushToast: vi.fn(() => "toast-id"),
    dismissToast: vi.fn(),
    recordMessage: vi.fn(async () => undefined),
  };
}
