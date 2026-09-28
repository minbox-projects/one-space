import { invoke } from "@tauri-apps/api/core";

const FALLBACK_INVOKE_ERROR_MESSAGE = "Toolbox command failed";

export class ToolboxInvokeError extends Error {
  readonly command: string;

  constructor(command: string, message: string) {
    super(message);
    this.name = "ToolboxInvokeError";
    this.command = command;
  }
}

export function isToolboxInvokeAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function normalizeInvokeErrorMessage(reason: unknown): string {
  if (typeof reason === "string" && reason.trim()) {
    return reason;
  }
  if (reason instanceof Error) {
    return reason.message.trim() ? reason.message : FALLBACK_INVOKE_ERROR_MESSAGE;
  }
  if (reason && typeof reason === "object") {
    const message = (reason as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) {
      return message;
    }
  }
  return FALLBACK_INVOKE_ERROR_MESSAGE;
}

export async function invokeToolboxCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isToolboxInvokeAvailable()) {
    throw new ToolboxInvokeError(
      command,
      "Toolbox commands are unavailable outside the Tauri runtime",
    );
  }

  try {
    return await invoke<T>(command, args);
  } catch (reason) {
    throw new ToolboxInvokeError(command, normalizeInvokeErrorMessage(reason));
  }
}
