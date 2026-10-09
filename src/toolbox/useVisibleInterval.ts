import { useEffect, useRef } from "react";

import { useAppVisibility } from "@/lib/runtimeStatus";

/**
 * Interval that only ticks while `enabled` AND the app is visible
 * (document + native window). On a hidden -> visible transition it invokes the
 * callback exactly once as a coalesced catch-up. Enabling the hook does not
 * invoke the callback immediately.
 */
export function useVisibleInterval(
  callback: () => void,
  intervalMs: number,
  enabled = true,
): void {
  const callbackRef = useRef(callback);
  const appVisible = useAppVisibility();
  const active = enabled && appVisible;
  const previousAppVisibleRef = useRef(appVisible);

  useEffect(() => {
    callbackRef.current = callback;
  }, [callback]);

  useEffect(() => {
    const wasAppVisible = previousAppVisibleRef.current;
    previousAppVisibleRef.current = appVisible;

    if (!active) return;

    // Hidden -> visible catch-up: exactly one callback for the elapsed hidden
    // period. Enabling the hook while already visible does not catch up.
    if (enabled && appVisible && !wasAppVisible) {
      callbackRef.current();
    }

    const timer = setInterval(() => {
      callbackRef.current();
    }, intervalMs);
    return () => clearInterval(timer);
  }, [active, appVisible, enabled, intervalMs]);
}
