import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

import { isToolboxInvokeAvailable } from "./invoke";

export function useTauriEvent(
  eventName: string,
  handler: (payload: unknown) => void,
  enabled = true,
): void {
  const handlerRef = useRef(handler);

  useEffect(() => {
    handlerRef.current = handler;
  }, [handler]);

  useEffect(() => {
    if (!enabled || !isToolboxInvokeAvailable()) return;

    let disposed = false;
    let unlisten: (() => void) | null = null;
    const wrapper = (event: { payload: unknown }) => {
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
  }, [enabled, eventName]);
}
