import { useEffect, useSyncExternalStore } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  AI_GATEWAY_TEMPLATE_AUTO_REFRESH_UPDATED_EVENT,
  aiGatewayTemplateAutoRefreshStatus,
  type TemplateAutoRefreshFailure,
} from "@/lib/aiGateway";

// ---------------------------------------------------------------------------
// Read/subscription adapter for the backend-owned template auto-refresh.
//
// The process scheduler owns timing, syncing, messages and failure state. This
// module only mirrors the backend failure snapshot so template cards can render
// inline failures: one status read on mount plus a subscription to the backend
// snapshot event. It never schedules, lists or syncs templates itself.
// ---------------------------------------------------------------------------

// In-memory, snapshot-stable failure store (template id -> reason).
let templateAutoRefreshFailures: Record<string, string> = {};
const templateAutoRefreshFailureListeners = new Set<() => void>();

function emitTemplateAutoRefreshFailures(): void {
  for (const listener of Array.from(templateAutoRefreshFailureListeners)) {
    listener();
  }
}

/** Replace the whole failure map from one backend snapshot. */
function replaceTemplateAutoRefreshFailures(
  failures: readonly TemplateAutoRefreshFailure[],
): void {
  const next: Record<string, string> = {};
  for (const failure of failures) {
    const templateId = failure?.template_id;
    if (typeof templateId !== "string" || templateId === "") continue;
    next[templateId] =
      typeof failure.reason === "string" ? failure.reason : String(failure.reason);
  }
  templateAutoRefreshFailures = next;
  emitTemplateAutoRefreshFailures();
}

function subscribeTemplateAutoRefreshFailures(listener: () => void): () => void {
  templateAutoRefreshFailureListeners.add(listener);
  return () => {
    templateAutoRefreshFailureListeners.delete(listener);
  };
}

function getTemplateAutoRefreshFailures(): Record<string, string> {
  return templateAutoRefreshFailures;
}

/** Stable snapshot of the current automatic-refresh failure reasons. */
export function useTemplateAutoRefreshFailures(): Record<string, string> {
  return useSyncExternalStore(
    subscribeTemplateAutoRefreshFailures,
    getTemplateAutoRefreshFailures,
    getTemplateAutoRefreshFailures,
  );
}

/** Extract a failure array from an IPC payload; `null` when malformed. */
function readFailureSnapshot(
  payload: unknown,
): TemplateAutoRefreshFailure[] | null {
  if (
    payload !== null &&
    typeof payload === "object" &&
    Array.isArray((payload as { failures?: unknown }).failures)
  ) {
    return (payload as { failures: TemplateAutoRefreshFailure[] }).failures;
  }
  return null;
}

/**
 * App-mounted read/subscription adapter. On mount it reads the backend failure
 * snapshot once and subscribes to the backend snapshot event, replacing the
 * whole map each time so cleared reasons disappear. Non-Tauri or failing calls
 * degrade to an empty map without breaking App.
 */
export function useTemplateAutoRefresh(): void {
  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;

    const readStatus = async () => {
      try {
        const status = await aiGatewayTemplateAutoRefreshStatus();
        if (disposed) return;
        replaceTemplateAutoRefreshFailures(status?.failures ?? []);
      } catch {
        // A missing/failing command leaves the adapter at an empty map.
        if (!disposed) replaceTemplateAutoRefreshFailures([]);
      }
    };

    void readStatus();

    void listen(AI_GATEWAY_TEMPLATE_AUTO_REFRESH_UPDATED_EVENT, (event) => {
      const failures = readFailureSnapshot(event.payload);
      // Malformed payloads keep the last good snapshot.
      if (failures === null) return;
      replaceTemplateAutoRefreshFailures(failures);
    })
      .then((stop) => {
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
      })
      .catch(() => {
        // Non-Tauri runtime: the status read already handles degradation.
      });

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
    };
  }, []);
}
