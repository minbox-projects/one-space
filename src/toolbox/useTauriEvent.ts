import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

import { isAppVisible } from "@/lib/runtimeStatus";
import { isToolboxInvokeAvailable } from "./invoke";

export type UseTauriEventOptions = {
  /**
   * Opt-in visibility-respecting mode. While the app is hidden the handler is
   * skipped (the subscription stays registered); the owning component/store is
   * responsible for one catch-up refresh when the app becomes visible again.
   */
  respectVisibility?: boolean;
};

export function useTauriEvent(
  eventName: string,
  handler: (payload: unknown) => void,
  enabled = true,
  options: UseTauriEventOptions = {},
): void {
  const handlerRef = useRef(handler);
  const respectVisibility = options.respectVisibility ?? false;

  useEffect(() => {
    handlerRef.current = handler;
  }, [handler]);

  useEffect(() => {
    if (!enabled || !isToolboxInvokeAvailable()) return;

    let disposed = false;
    let unlisten: (() => void) | null = null;
    const wrapper = (event: { payload: unknown }) => {
      if (respectVisibility && !isAppVisible()) return;
      handlerRef.current(event.payload);
    };

    listen(eventName, wrapper)
      .then((stop) => {
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
      })
      .catch(() => {
        // Subscription failures must never crash the tool view.
      });

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
    };
  }, [enabled, eventName, respectVisibility]);
}
