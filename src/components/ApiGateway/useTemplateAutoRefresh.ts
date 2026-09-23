import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import {
  apiGatewayProviderTemplates,
  apiGatewaySyncProviderTemplate,
  apiGatewayTemplateAutoRefreshGet,
  subscribeTemplateAutoRefreshIntervalChanged,
} from "@/lib/apiGateway";

// ---------------------------------------------------------------------------
// Manual-sync in-flight registry
// ---------------------------------------------------------------------------

const templateSyncInFlightIds = new Set<string>();

/** Mark whether a template currently has a manual sync running. */
export function setTemplateSyncInFlight(
  templateId: string,
  inFlight: boolean,
): void {
  if (inFlight) {
    templateSyncInFlightIds.add(templateId);
  } else {
    templateSyncInFlightIds.delete(templateId);
  }
}

/** Whether the given template has a manual sync in flight. */
export function isTemplateSyncInFlight(templateId: string): boolean {
  return templateSyncInFlightIds.has(templateId);
}

/** Drop every manual-sync in-flight marker. */
export function clearTemplateSyncInFlight(): void {
  templateSyncInFlightIds.clear();
}

// ---------------------------------------------------------------------------
// Automatic-refresh failure store (in-memory, snapshot-stable)
// ---------------------------------------------------------------------------

let templateAutoRefreshFailures: Record<string, string> = {};
const templateAutoRefreshFailureListeners = new Set<() => void>();

function emitTemplateAutoRefreshFailures(): void {
  for (const listener of Array.from(templateAutoRefreshFailureListeners)) {
    listener();
  }
}

/**
 * Record the inline failure reason for one template. A `null` reason clears it;
 * a missing template with a `null` reason is a no-op.
 */
export function setTemplateAutoRefreshFailure(
  templateId: string,
  reason: string | null,
): void {
  if (reason === null) {
    if (!(templateId in templateAutoRefreshFailures)) return;
    const next = { ...templateAutoRefreshFailures };
    delete next[templateId];
    templateAutoRefreshFailures = next;
  } else {
    if (templateAutoRefreshFailures[templateId] === reason) return;
    templateAutoRefreshFailures = {
      ...templateAutoRefreshFailures,
      [templateId]: reason,
    };
  }
  emitTemplateAutoRefreshFailures();
}

/** Clear every automatic-refresh failure reason. */
export function clearTemplateAutoRefreshFailures(): void {
  if (Object.keys(templateAutoRefreshFailures).length === 0) return;
  templateAutoRefreshFailures = {};
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

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

/** Only a positive integer interval enables the schedule. */
function normalizeTemplateAutoRefreshInterval(value: unknown): number | null {
  if (typeof value !== "number") return null;
  if (!Number.isFinite(value) || !Number.isInteger(value)) return null;
  if (value <= 0) return null;
  return value;
}

function describeAutoRefreshFailure(value: unknown): string {
  if (value instanceof Error) return value.message;
  return String(value);
}

/**
 * App-mounted scheduler that refreshes every URL-backed provider template on
 * the persisted interval. It reads the interval once on mount, recomputes the
 * timer whenever the persisted value changes, skips overlapping ticks, defers
 * templates with an in-flight manual sync and records per-template failure
 * reasons in memory without toasting.
 */
export function useTemplateAutoRefresh(): void {
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const batchInFlightRef = useRef(false);

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const runBatch = useCallback(async () => {
    if (batchInFlightRef.current) return;
    batchInFlightRef.current = true;
    try {
      let views;
      try {
        views = await apiGatewayProviderTemplates();
      } catch {
        // A list failure is swallowed: no toast, no partial batch.
        return;
      }
      if (!Array.isArray(views)) return;

      for (const view of views) {
        const templateId = view?.template?.id;
        if (!templateId) continue;
        if (!view.template.models_url?.trim()) continue;
        if (isTemplateSyncInFlight(templateId)) continue;

        try {
          await apiGatewaySyncProviderTemplate(templateId);
          setTemplateAutoRefreshFailure(templateId, null);
        } catch (error) {
          setTemplateAutoRefreshFailure(
            templateId,
            describeAutoRefreshFailure(error),
          );
        }
      }
    } finally {
      batchInFlightRef.current = false;
    }
  }, []);

  const applyInterval = useCallback(
    (value: unknown) => {
      clearTimer();
      const minutes = normalizeTemplateAutoRefreshInterval(value);
      if (minutes === null) return;
      timerRef.current = setInterval(() => {
        void runBatch();
      }, minutes * 60_000);
    },
    [clearTimer, runBatch],
  );

  useEffect(() => {
    let cancelled = false;

    const readAndApplyInterval = async () => {
      try {
        const minutes = await apiGatewayTemplateAutoRefreshGet();
        if (cancelled) return;
        applyInterval(minutes);
      } catch {
        // A read failure leaves the schedule stopped.
        if (!cancelled) applyInterval(0);
      }
    };

    void readAndApplyInterval();

    const unsubscribe = subscribeTemplateAutoRefreshIntervalChanged(() => {
      void readAndApplyInterval();
    });

    return () => {
      cancelled = true;
      unsubscribe();
      clearTimer();
    };
  }, [applyInterval, clearTimer]);
}
