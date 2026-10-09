import { useCallback, useEffect, useSyncExternalStore } from "react";

import { aiGatewayStatus } from "@/lib/aiGateway";
import { protocolRouterStatus } from "@/lib/protocolRouter";
import { fileSharingStatus } from "@/lib/fileSharing";
import { sshTunnelsSnapshot } from "@/lib/sshTunnels";

/**
 * Shared runtime-status store (REQ-004 / AC-004).
 *
 * One WebView-wide store per service. A valid snapshot payload published from a
 * backend event is consumed directly with zero follow-up queries; a missing or
 * unit payload triggers at most one coalesced in-flight fallback pull per
 * service. Consumers subscribe through {@link useRuntimeStatus} and never pull
 * independently on events.
 */
export type RuntimeService =
  | "gateway"
  | "router"
  | "ssh-tunnels"
  | "file-sharing";

export type RuntimeStatusSnapshot<T> = {
  data: T | null;
  loading: boolean;
  error: unknown;
};

type ServiceEntry = {
  data: unknown;
  loading: boolean;
  error: unknown;
  hasData: boolean;
  snapshot: RuntimeStatusSnapshot<unknown>;
  listeners: Set<() => void>;
  inFlight: Promise<unknown> | null;
  inFlightToken: object | null;
  seq: number;
  fallbackQueued: boolean;
};

const COMMANDS: Record<RuntimeService, () => Promise<unknown>> = {
  gateway: () => aiGatewayStatus(),
  router: () => protocolRouterStatus(),
  "ssh-tunnels": () => sshTunnelsSnapshot(),
  "file-sharing": () => fileSharingStatus(),
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

const VALIDATORS: Record<RuntimeService, (payload: unknown) => boolean> = {
  gateway: (payload) => isRecord(payload) && typeof payload.running === "boolean",
  router: (payload) =>
    isRecord(payload) &&
    typeof payload.running === "boolean" &&
    typeof payload.enabled === "boolean",
  "ssh-tunnels": (payload) =>
    isRecord(payload) && Array.isArray(payload.tunnels),
  "file-sharing": (payload) =>
    isRecord(payload) &&
    typeof payload.running === "boolean" &&
    Array.isArray(payload.files),
};

const entries = new Map<RuntimeService, ServiceEntry>();

function createEntry(): ServiceEntry {
  return {
    data: null,
    loading: false,
    error: null,
    hasData: false,
    snapshot: { data: null, loading: false, error: null },
    listeners: new Set(),
    inFlight: null,
    inFlightToken: null,
    seq: 0,
    fallbackQueued: false,
  };
}

function getEntry(service: RuntimeService): ServiceEntry {
  let entry = entries.get(service);
  if (!entry) {
    entry = createEntry();
    entries.set(service, entry);
  }
  return entry;
}

function emit(entry: ServiceEntry): void {
  const previous = entry.snapshot;
  if (
    previous.data === entry.data &&
    previous.loading === entry.loading &&
    previous.error === entry.error
  ) {
    return;
  }
  entry.snapshot = {
    data: entry.data,
    loading: entry.loading,
    error: entry.error,
  };
  for (const listener of entry.listeners) {
    listener();
  }
}

function subscribeService(
  service: RuntimeService,
  listener: () => void,
): () => void {
  const entry = getEntry(service);
  entry.listeners.add(listener);
  return () => {
    entry.listeners.delete(listener);
  };
}

function scheduleFallback(service: RuntimeService): void {
  const entry = getEntry(service);
  if (entry.fallbackQueued) return;
  entry.fallbackQueued = true;
  queueMicrotask(() => {
    entry.fallbackQueued = false;
    void refreshRuntimeStatus(service);
  });
}

/**
 * Consume a backend runtime-status payload. A valid snapshot is stored with no
 * extra query; an invalid or missing payload schedules one coalesced fallback
 * pull shared by every consumer of that service.
 */
export function publishRuntimeStatus(
  service: RuntimeService,
  payload: unknown,
): boolean {
  const entry = getEntry(service);
  if (VALIDATORS[service](payload)) {
    entry.seq += 1;
    entry.data = payload;
    entry.hasData = true;
    entry.error = null;
    entry.loading = false;
    emit(entry);
    return true;
  }
  scheduleFallback(service);
  return false;
}

/**
 * Single-flight status refresh. The sequence guard prevents a late older
 * response from overwriting newer state.
 */
export function refreshRuntimeStatus<T = unknown>(
  service: RuntimeService,
  options: { force?: boolean } = {},
): Promise<T | null> {
  const entry = getEntry(service);
  if (entry.inFlight && !options.force) {
    return entry.inFlight as Promise<T | null>;
  }

  const requestId = ++entry.seq;
  const token = {};
  entry.loading = true;
  emit(entry);

  const promise = (async () => {
    try {
      const data = await COMMANDS[service]();
      if (requestId !== entry.seq) {
        return entry.data as T | null;
      }
      entry.data = data;
      entry.hasData = true;
      entry.error = null;
      entry.loading = false;
      emit(entry);
      return data as T;
    } catch (error) {
      if (requestId === entry.seq) {
        entry.error = error;
        entry.loading = false;
        emit(entry);
      }
      return entry.data as T | null;
    } finally {
      if (entry.inFlightToken === token) {
        entry.inFlight = null;
        entry.inFlightToken = null;
      }
    }
  })();

  entry.inFlight = promise as Promise<unknown>;
  entry.inFlightToken = token;
  return promise as Promise<T | null>;
}

/**
 * Subscribe to one shared runtime-status service. The hook performs a
 * single-flight startup pull on mount so a fresh mount (or a page that owns the
 * service alone) always sees current state; consumers do not pull per event.
 */
export function useRuntimeStatus<T = unknown>(
  service: RuntimeService,
  options: { enabled?: boolean } = {},
) {
  const { enabled = true } = options;
  const subscribe = useCallback(
    (listener: () => void) => subscribeService(service, listener),
    [service],
  );
  const getSnapshot = useCallback(
    () => getEntry(service).snapshot as RuntimeStatusSnapshot<T>,
    [service],
  );
  const snapshot = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const refresh = useCallback(
    (refreshOptions?: { force?: boolean }) =>
      refreshRuntimeStatus<T>(service, refreshOptions),
    [service],
  );

  useEffect(() => {
    if (!enabled) return;
    void refreshRuntimeStatus<T>(service);
  }, [enabled, service]);

  return {
    data: snapshot.data,
    loading: snapshot.loading,
    error: snapshot.error,
    refresh,
  };
}

// ---------------------------------------------------------------------------
// Combined document + native-window visibility (REQ-004 / AC-004).
// ---------------------------------------------------------------------------

function isDocumentVisible(): boolean {
  if (typeof document === "undefined") return true;
  return document.visibilityState !== "hidden";
}

let nativeWindowVisible = true;
let lastNotifiedAppVisible = nativeWindowVisible && isDocumentVisible();
const visibilityListeners = new Set<() => void>();

function computeAppVisible(): boolean {
  return nativeWindowVisible && isDocumentVisible();
}

function notifyVisibility(): void {
  const next = computeAppVisible();
  if (next === lastNotifiedAppVisible) return;
  lastNotifiedAppVisible = next;
  for (const listener of visibilityListeners) {
    listener();
  }
}

let documentVisibilityListenerAttached = false;

function attachDocumentVisibilityListener(): void {
  if (documentVisibilityListenerAttached || typeof document === "undefined") {
    return;
  }
  documentVisibilityListenerAttached = true;
  document.addEventListener("visibilitychange", notifyVisibility);
}

function subscribeVisibility(listener: () => void): () => void {
  attachDocumentVisibilityListener();
  visibilityListeners.add(listener);
  return () => {
    visibilityListeners.delete(listener);
  };
}

/** Native main-window visibility flag, driven by `main-window-visibility-changed`. */
export function setNativeWindowVisible(visible: boolean): void {
  if (nativeWindowVisible === visible) return;
  nativeWindowVisible = visible;
  notifyVisibility();
}

/** Current combined visibility: document visible AND native window visible. */
export function isAppVisible(): boolean {
  return computeAppVisible();
}

/** Subscribe to combined document + native-window visibility. */
export function useAppVisibility(): boolean {
  return useSyncExternalStore(subscribeVisibility, computeAppVisible, () => true);
}
