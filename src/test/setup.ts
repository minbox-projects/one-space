import "@testing-library/jest-dom/vitest";
import { afterAll, afterEach, beforeAll, vi } from "vitest";
import { cleanup } from "@testing-library/react";

const originalConsoleInfo = console.info;
const originalConsoleError = console.error;

const shouldIgnoreI18nextSupportNotice = (value: unknown) =>
  typeof value === "string" &&
  value.includes("i18next is maintained with support from Locize");

const shouldIgnoreExpectedTestError = (value: unknown) =>
  (typeof value === "string" &&
    (value.startsWith("Failed to restore backup") ||
      value.includes("permission denied"))) ||
  (value instanceof Error &&
    (value.message === "permission denied" || value.message === "restore failed"));

console.info = (...args: unknown[]) => {
  if (args.some(shouldIgnoreI18nextSupportNotice)) {
    return;
  }
  originalConsoleInfo(...args);
};

console.error = (...args: unknown[]) => {
  if (args.some(shouldIgnoreExpectedTestError)) {
    return;
  }
  originalConsoleError(...args);
};

await import("@/i18n");
await import("@/test/mocks/tauri");
await import("@/test/mocks/messages");

beforeAll(() => {
  console.info = console.info;
  console.error = console.error;
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

afterAll(() => {
  console.info = originalConsoleInfo;
  console.error = originalConsoleError;
});

Object.defineProperty(window, "__TAURI_INTERNALS__", {
  value: {},
  configurable: true,
});
