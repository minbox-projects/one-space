export const AI_GATEWAY_STATUS_UPDATED_EVENT = "ai-gateway-status-update";

export const AI_GATEWAY_CONFIG_UPDATED_EVENT = "ai-gateway-config-update";

/** Runtime broadcast emitted when a key transitions into the authentication-failed mark. */
export const AI_GATEWAY_KEY_AUTH_FAILED_EVENT = "ai-gateway-key-auth-failed";

/** Per-provider aggregation window for authentication-failed toasts. */
export const AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS = 1500;

/** Delay before an unacknowledged provider-card warning pill auto-dismisses. */
export const AI_GATEWAY_ALERT_AUTO_DISMISS_MS = 8000;


// Subscribers notified after the template auto-refresh interval is persisted so
// long-lived views can re-read the value instead of polling or prop drilling.
const templateAutoRefreshIntervalListeners = new Set<() => void>();


export function notifyTemplateAutoRefreshIntervalChanged(): void {
  for (const listener of Array.from(templateAutoRefreshIntervalListeners)) {
    listener();
  }
}


export function subscribeTemplateAutoRefreshIntervalChanged(
  listener: () => void,
): () => void {
  templateAutoRefreshIntervalListeners.add(listener);
  return () => {
    templateAutoRefreshIntervalListeners.delete(listener);
  };
}


/**
 * Tauri event literal emitted after the backend automatic-refresh failure set
 * changes; its payload is a {@link TemplateAutoRefreshStatus}.
 */
export const AI_GATEWAY_TEMPLATE_AUTO_REFRESH_UPDATED_EVENT =
  "ai-gateway-template-auto-refresh-updated";
