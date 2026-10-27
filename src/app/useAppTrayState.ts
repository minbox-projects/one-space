import { useRef, useState } from "react";
import type { TrayMenuState } from "@/lib/trayMenu";

export function useAppTrayState() {
  const [trayState, setTrayState] = useState<TrayMenuState>({
    windowVisible: true,
    gateway: { running: false },
    router: { running: false },
    tunnels: { connected: 0, total: 0 },
    sharing: { running: false, fileCount: 0 },
    shortcuts: { main: null, quick: null },
  });
  const trayStateRef = useRef(trayState);
  const trayMenuApplyRef = useRef<Promise<void>>(Promise.resolve());
  const trayActionRef = useRef<(id: string) => Promise<void>>(async () => {});
  const gatewayBaseUrlRef = useRef<string | null>(null);
  const sshTunnelSummaryRef = useRef<{
    connectedCount: number;
    hasErrors: boolean;
    hasConnecting: boolean;
    errorTunnelNames: string[];
  } | null>(null);
  const sshReconnectLoadingToastRef = useRef<string | null>(null);

  return {
    trayState,
    setTrayState,
    trayStateRef,
    trayMenuApplyRef,
    trayActionRef,
    gatewayBaseUrlRef,
    sshTunnelSummaryRef,
    sshReconnectLoadingToastRef,
  };
}
