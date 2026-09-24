import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import i18n from "@/i18n";
import {
  aiGatewayGetConfig,
  aiGatewayProviderTemplates,
  aiGatewaySyncProviderTemplate,
  aiGatewayTemplateAutoRefreshGet,
  subscribeTemplateAutoRefreshIntervalChanged,
  type GatewayConfig,
  type GatewayProviderTemplateView,
} from "@/lib/aiGateway";
import { safeRecordMessage, type MessageCreateInput } from "@/lib/messages";

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

/** Aggregated qualifying changes for one template's bound providers. */
interface TemplateSyncChange {
  affectedProviderCount: number;
  addedCount: number;
  disabledCount: number;
  detail: string;
}

/**
 * Diff the providers bound to one template between two gateway configurations.
 *
 * Only model-mapping additions (an `upstream_model` absent before) and enables
 * that turned into an explicit `false` qualify; field-only differences and
 * providers not bound to the template produce nothing. Returns `null` when no
 * provider qualified.
 */
function computeTemplateSyncChange(
  previous: GatewayConfig,
  current: GatewayConfig,
  templateId: string,
): TemplateSyncChange | null {
  const affected: Array<{ name: string; identifiers: string[] }> = [];
  let addedCount = 0;
  let disabledCount = 0;

  for (const provider of current.providers) {
    if (provider.template_id !== templateId) continue;
    const previousProvider = previous.providers.find(
      (candidate) => candidate.id === provider.id,
    );
    const previousByUpstream = new Map(
      (previousProvider?.mappings ?? []).map((mapping) => [
        mapping.upstream_model,
        mapping,
      ]),
    );

    const identifiers: string[] = [];
    for (const mapping of provider.mappings) {
      const before = previousByUpstream.get(mapping.upstream_model);
      if (before === undefined) {
        addedCount += 1;
      } else if (before.enabled !== false && mapping.enabled === false) {
        disabledCount += 1;
      } else {
        continue;
      }
      const localModel = mapping.local_model.trim();
      identifiers.push(localModel !== "" ? localModel : mapping.upstream_model);
    }

    if (identifiers.length > 0) {
      affected.push({ name: provider.name, identifiers });
    }
  }

  if (affected.length === 0) return null;

  return {
    affectedProviderCount: affected.length,
    addedCount,
    disabledCount,
    detail: affected
      .map(({ name, identifiers }) =>
        i18n.t("aiGatewayTemplateSyncNotificationDetailProvider", {
          provider: name,
          models: identifiers.join(", "),
        }),
      )
      .join("\n"),
  };
}

/** Build the single message-center payload for one changed template. */
function buildTemplateSyncMessage(
  view: GatewayProviderTemplateView,
  change: TemplateSyncChange,
): MessageCreateInput {
  const parts = [
    i18n.t("aiGatewayTemplateSyncNotificationProviderCount", {
      count: change.affectedProviderCount,
    }),
  ];
  if (change.addedCount > 0) {
    parts.push(
      i18n.t("aiGatewayTemplateSyncNotificationAddedCount", {
        count: change.addedCount,
      }),
    );
  }
  if (change.disabledCount > 0) {
    parts.push(
      i18n.t("aiGatewayTemplateSyncNotificationDisabledCount", {
        count: change.disabledCount,
      }),
    );
  }

  return {
    source: "ai_gateway",
    category: "template_sync",
    severity: "info",
    title: i18n.t("aiGatewayTemplateSyncNotificationTitle", {
      template: view.template.name,
    }),
    summary: parts.join("; "),
    detail: change.detail,
    target: { tab: "ai-gateway" },
  };
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
        views = await aiGatewayProviderTemplates();
      } catch {
        // A list failure is swallowed: no toast, no partial batch.
        return;
      }
      if (!Array.isArray(views)) return;

      // Snapshot the gateway configuration once before the per-template loop so
      // each successful sync can be compared against the pre-batch state. A
      // failure here suppresses every notification in the batch but never
      // blocks the syncs themselves.
      let previous: GatewayConfig | null = null;
      try {
        previous = await aiGatewayGetConfig();
      } catch {
        previous = null;
      }

      for (const view of views) {
        const templateId = view?.template?.id;
        if (!templateId) continue;
        if (!view.template.models_url?.trim()) continue;
        if (isTemplateSyncInFlight(templateId)) continue;

        try {
          await aiGatewaySyncProviderTemplate(templateId);
          setTemplateAutoRefreshFailure(templateId, null);
        } catch (error) {
          setTemplateAutoRefreshFailure(
            templateId,
            describeAutoRefreshFailure(error),
          );
          continue;
        }

        // No usable pre-batch baseline: keep syncing, never notify this batch.
        if (previous === null) continue;

        let current: GatewayConfig;
        try {
          current = await aiGatewayGetConfig();
        } catch {
          // A read failure suppresses only this template's notification and
          // leaves the baseline untouched for the next template.
          continue;
        }

        const change = computeTemplateSyncChange(previous, current, templateId);
        previous = current;
        if (!change) continue;

        try {
          await safeRecordMessage(buildTemplateSyncMessage(view, change));
        } catch {
          // A message-store failure suppresses only this template's message.
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
        const minutes = await aiGatewayTemplateAutoRefreshGet();
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
