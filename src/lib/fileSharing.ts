import { invoke } from "@tauri-apps/api/core";

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

export type FileSharingPublicState =
  | "disabled"
  | "starting"
  | "running"
  | "error";

export type FileSharingStartInput = {
  networkId: string;
  paths: string[];
  publicAccess?: boolean;
  password?: string;
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
  fileName: string;
  clientAddress: string;
  state: FileSharingTransferState;
  bytesSent: number;
};

export type FileSharingSnapshot = {
  running: boolean;
  sessionId: string | null;
  shareUrl: string | null;
  startedAt: number | null;
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
  passwordProtected?: boolean;
  publicTunnelId?: string | null;
  publicState?: FileSharingPublicState;
  publicShareUrl?: string | null;
  publicError?: string | null;
};

export const FILE_SHARING_PUBLIC_UPDATED_EVENT = "file-sharing-public-updated";

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
  const payload: FileSharingStartInput = {
    networkId: input.networkId,
    paths: input.paths,
  };
  if (input.publicAccess !== undefined) payload.publicAccess = input.publicAccess;
  if (input.password !== undefined) payload.password = input.password;
  return fileSharingInvoke<FileSharingSnapshot>("file_sharing_start", { input: payload });
}

export function fileSharingStatus() {
  return fileSharingInvoke<FileSharingSnapshot>("file_sharing_status");
}

export function fileSharingStop() {
  return fileSharingInvoke<FileSharingSnapshot>("file_sharing_stop");
}

export function fileSharingPublicStart() {
  return fileSharingInvoke<FileSharingSnapshot>("file_sharing_public_start");
}

export function fileSharingPublicStop() {
  return fileSharingInvoke<FileSharingSnapshot>("file_sharing_public_stop");
}

/**
 * The public link is usable only while the share-owned tunnel reports running;
 * every other state falls back to the LAN link so the UI never advertises a
 * dead public endpoint.
 */
export function getFileSharingPublicUrl(
  snapshot: Pick<FileSharingSnapshot, "publicState" | "publicShareUrl" | "shareUrl">,
): string | null {
  if (snapshot.publicState === "running" && snapshot.publicShareUrl) {
    return snapshot.publicShareUrl;
  }
  return snapshot.shareUrl;
}
