import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";

export type FileSharingTransferState =
  | "in_progress"
  | "completed"
  | "client_disconnected"
  | "cancelled"
  | "failed";

export type FileSharingNetwork = {
  id: string;
  interfaceName: string;
  address: string;
};

export type FileSharingStartInput = {
  networkId: string;
  paths: string[];
};

export type FileSharingFile = {
  id: string;
  name: string;
  sourcePath: string;
  size: number;
  modifiedAt: number;
};

export type FileSharingTransfer = {
  id: string;
  fileId: string;
  fileName: string;
  clientAddress: string;
  state: FileSharingTransferState;
  startedAt: number;
  finishedAt: number | null;
  bytesSent: number;
  responseBytes: number;
  error: string | null;
};

export type FileSharingSnapshot = {
  running: boolean;
  sessionId: string | null;
  address: string | null;
  port: number | null;
  shareUrl: string | null;
  startedAt: number | null;
  stoppedAt: number | null;
  files: FileSharingFile[];
  transfers: FileSharingTransfer[];
  summary: {
    activeTransfers: number;
    completedTransfers: number;
    failedTransfers: number;
    cancelledTransfers: number;
    bytesSent: number;
    droppedTransferRecords: number;
  };
  lastError: string | null;
};

export type FileSharingUpdate = { kind: "session" | "transfer" };

export class FileSharingError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "FileSharingError";
  }
}

function normalizeError(error: unknown) {
  if (error instanceof FileSharingError) return error;
  if (error instanceof Error) return new FileSharingError(error.message);
  if (typeof error === "string") return new FileSharingError(error);
  try {
    return new FileSharingError(JSON.stringify(error) || "File sharing command failed");
  } catch {
    return new FileSharingError("File sharing command failed");
  }
}

async function fileSharingInvoke<T>(command: string, args?: Record<string, unknown>) {
  try {
    return args ? await invoke<T>(command, args) : await invoke<T>(command);
  } catch (error) {
    throw normalizeError(error);
  }
}

export function fileSharingNetworks() {
  return fileSharingInvoke<FileSharingNetwork[]>("file_sharing_networks");
}

export function fileSharingStart(input: FileSharingStartInput) {
  return fileSharingInvoke<FileSharingSnapshot>("file_sharing_start", { input });
}

export function fileSharingStatus() {
  return fileSharingInvoke<FileSharingSnapshot>("file_sharing_status");
}

export function fileSharingStop() {
  return fileSharingInvoke<FileSharingSnapshot>("file_sharing_stop");
}

export function subscribeFileSharingUpdates(
  handler: (payload: FileSharingUpdate) => void,
) {
  return listen<FileSharingUpdate>("file-sharing-updated", (event: Event<FileSharingUpdate>) => {
    handler(event.payload);
  });
}
