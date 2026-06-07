import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import "@/i18n";
import "@/test/mocks/tauri";
import "@/test/mocks/messages";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

Object.defineProperty(window, "__TAURI_INTERNALS__", {
  value: {},
  configurable: true,
});
