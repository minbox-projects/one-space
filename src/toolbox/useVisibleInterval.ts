import { useEffect, useRef } from "react";

export function useVisibleInterval(
  callback: () => void,
  intervalMs: number,
  enabled = true,
): void {
  const callbackRef = useRef(callback);

  useEffect(() => {
    callbackRef.current = callback;
  }, [callback]);

  useEffect(() => {
    if (!enabled) return;

    const timer = setInterval(() => {
      callbackRef.current();
    }, intervalMs);
    return () => clearInterval(timer);
  }, [enabled, intervalMs]);
}
