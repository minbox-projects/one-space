import { useCallback, useEffect } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import { useRuntimeStatus } from "@/lib/runtimeStatus";
import type { GatewayStatus } from "@/lib/aiGateway";
import type { ProtocolRouterStatus } from "@/lib/protocolRouter";
import type { SshTunnelsSnapshot } from "@/components/sshTunnels/types";
import type { FileSharingSnapshot } from "@/lib/fileSharing";
import type { TunnelRuntimeStatus } from "@/lib/portTunnel";
import type { TrayMenuState } from "@/lib/trayMenu";
import type { BleLockStatus } from "@/lib/bleLock";
import type { MessageCreateInput } from "@/lib/messages";
import { deriveSshTunnelHeaderSummary } from "@/lib/sshTunnelSummary";
import { derivePortTunnelHeaderSummary } from "@/lib/portTunnelSummary";
import {
  buildSshUnexpectedDisconnectEvent,
} from "@/lib/actionDescriptors/appSystemEvents";
import { notifySystemEvent } from "@/lib/userActions";

type UseAppRuntimeSummaryParams = {
  runtimeEnabled: boolean;
  gatewayBaseUrlRef: { current: string | null };
  sshTunnelSummaryRef: {
    current: {
      connectedCount: number;
      hasErrors: boolean;
      hasConnecting: boolean;
      errorTunnelNames: string[];
    } | null;
  };
  setProtocolRouterHeaderStatus: (status: ProtocolRouterStatus | null) => void;
  setAiGatewayHeaderStatus: (status: GatewayStatus | null) => void;
  setFileSharingHeaderStatus: (status: {
    running: boolean;
    fileCount: number;
  } | null) => void;
  setSshTunnelSummary: (
    summary: {
      connectedCount: number;
      hasErrors: boolean;
      hasConnecting: boolean;
      errorTunnelNames: string[];
    } | null,
  ) => void;
  setTrayState: Dispatch<SetStateAction<TrayMenuState>>;
  pushToast: (toast: {
    title: string;
    description?: string;
    kind?: "info" | "success" | "warning" | "error" | "loading";
  }) => string;
  recordMessage: (message: MessageCreateInput) => Promise<void>;
  t: TFunction;
};

export function useAppRuntimeSummary({
  runtimeEnabled,
  gatewayBaseUrlRef,
  sshTunnelSummaryRef,
  setProtocolRouterHeaderStatus,
  setAiGatewayHeaderStatus,
  setFileSharingHeaderStatus,
  setSshTunnelSummary,
  setTrayState,
  pushToast,
  recordMessage,
  t,
}: UseAppRuntimeSummaryParams) {
  // Shared runtime-status store: one coalesced query per service per WebView.
  // The App event bridge below only publishes backend payloads; header and tray
  // derive from this single subscription instead of pulling on every event.
  const gatewayRuntime = useRuntimeStatus<GatewayStatus>("gateway", {
    enabled: runtimeEnabled,
  });
  const routerRuntime = useRuntimeStatus<ProtocolRouterStatus>("router", {
    enabled: runtimeEnabled,
  });
  const tunnelsRuntime = useRuntimeStatus<SshTunnelsSnapshot>("ssh-tunnels", {
    enabled: runtimeEnabled,
  });
  const sharingRuntime = useRuntimeStatus<FileSharingSnapshot>("file-sharing", {
    enabled: runtimeEnabled,
  });
  const portTunnelRuntime = useRuntimeStatus<TunnelRuntimeStatus[]>("port-tunnel", {
    enabled: runtimeEnabled,
  });
  const portTunnelSummary = derivePortTunnelHeaderSummary(portTunnelRuntime.data || []);

  useEffect(() => {
    const status = gatewayRuntime.data;
    if (!status) return;
    gatewayBaseUrlRef.current = status.local_base_url;
    setAiGatewayHeaderStatus(status);
    setTrayState((prev) =>
      prev.gateway.running === status.running && prev.gateway.port === status.port
        ? prev
        : { ...prev, gateway: { running: status.running, port: status.port } },
    );
  }, [gatewayRuntime.data]);

  useEffect(() => {
    const status = routerRuntime.data;
    if (!status) return;
    setProtocolRouterHeaderStatus(status);
    setTrayState((prev) =>
      prev.router.running === status.running && prev.router.port === status.port
        ? prev
        : { ...prev, router: { running: status.running, port: status.port } },
    );
  }, [routerRuntime.data]);

  useEffect(() => {
    const snapshot = tunnelsRuntime.data;
    if (!snapshot) return;
    const summary = deriveSshTunnelHeaderSummary(snapshot);
    const previousErrors = sshTunnelSummaryRef.current?.errorTunnelNames ?? [];
    const newErrors = summary.errorTunnelNames.filter(
      (name) => !previousErrors.includes(name),
    );
    for (const name of newErrors) {
      void notifySystemEvent(
        { pushToast, recordMessage },
        buildSshUnexpectedDisconnectEvent(t, name),
      );
    }
    sshTunnelSummaryRef.current = summary;
    setSshTunnelSummary(summary);
    const runtimeEntries = Array.isArray(snapshot.runtime) ? snapshot.runtime : [];
    const total = Array.isArray(snapshot.tunnels) ? snapshot.tunnels.length : 0;
    const connected = runtimeEntries.filter(
      (entry) => entry.status === "connected",
    ).length;
    setTrayState((prev) =>
      prev.tunnels.connected === connected && prev.tunnels.total === total
        ? prev
        : { ...prev, tunnels: { connected, total } },
    );
  }, [tunnelsRuntime.data, pushToast, recordMessage, t]);

  useEffect(() => {
    const snapshot = sharingRuntime.data;
    if (!snapshot) return;
    const sharing = {
      running: snapshot.running,
      fileCount: (snapshot.files ?? []).length,
    };
    setFileSharingHeaderStatus(sharing);
    setTrayState((prev) =>
      prev.sharing.running === sharing.running &&
      prev.sharing.fileCount === sharing.fileCount
        ? prev
        : { ...prev, sharing },
    );
  }, [sharingRuntime.data]);

  // The tray-only BLE Lock projection lives in a dedicated null-render child
  // (TrayBleLockBridge) so a live RSSI stream never re-renders the whole shell.
  // This callback is stable and is only invoked when a tray-visible field moves.
  // setTrayState is a stable useState setter, so the original empty dependency
  // list keeps this callback stable.
  // eslint-disable-next-line react-hooks/preserve-manual-memoization
  const handleTrayBleLockStatus = useCallback((status: BleLockStatus) => {
    setTrayState((prev) =>
      prev.bleLock === status ? prev : { ...prev, bleLock: status },
    );
  }, []);

  return {
    gatewayRuntime,
    routerRuntime,
    tunnelsRuntime,
    sharingRuntime,
    portTunnelRuntime,
    portTunnelSummary,
    handleTrayBleLockStatus,
  };
}
