import { useState } from "react";
import type { ProtocolRouterStatus } from "@/lib/protocolRouter";
import type { GatewayStatus } from "@/lib/aiGateway";
import type { MoreToolsSection } from "@/lib/navigation";
import type { JttParserTab } from "@/lib/navigation";

export function useAppViewState() {
  const [messageUnreadCount, setMessageUnreadCount] = useState(0);
  const [moreToolsSection, setMoreToolsSection] =
    useState<MoreToolsSection | null>(null);
  const [jttParserTab, setJttParserTab] = useState<JttParserTab | null>(null);
  const [sshTunnelTab, setSshTunnelTab] = useState<string | null>(null);
  const [sshTunnelNavigationNonce, setSshTunnelNavigationNonce] = useState(0);
  const [moreToolsReturnTab, setMoreToolsReturnTab] = useState<
    "launcher" | "more-tools"
  >("more-tools");
  const [storageType, setStorageType] = useState<"local" | "git" | "icloud">(
    "local",
  );
  const [onboardingStatus, setOnboardingStatus] = useState<
    "checking" | "required" | "done"
  >("checking");
  const [mountedTabs, setMountedTabs] = useState<Set<string>>(
    () => new Set(["launcher"]),
  );

  // Git Sync Status
  const [syncStatus, setSyncStatus] = useState<
    "idle" | "pulling" | "pushing" | "success" | "error"
  >("idle");
  const [syncError, setSyncError] = useState<string | null>(null);
  const [networkCircuitOpen, setNetworkCircuitOpen] = useState(false);
  const [sshTunnelSummary, setSshTunnelSummary] = useState<{
    connectedCount: number;
    hasErrors: boolean;
    hasConnecting: boolean;
    errorTunnelNames: string[];
  } | null>(null);
  const [protocolRouterHeaderStatus, setProtocolRouterHeaderStatus] =
    useState<ProtocolRouterStatus | null>(null);
  const [aiGatewayHeaderStatus, setAiGatewayHeaderStatus] =
    useState<GatewayStatus | null>(null);
  const [fileSharingHeaderStatus, setFileSharingHeaderStatus] = useState<{
    running: boolean;
    fileCount: number;
  } | null>(null);

  return {
    messageUnreadCount,
    setMessageUnreadCount,
    moreToolsSection,
    setMoreToolsSection,
    jttParserTab,
    setJttParserTab,
    sshTunnelTab,
    setSshTunnelTab,
    sshTunnelNavigationNonce,
    setSshTunnelNavigationNonce,
    moreToolsReturnTab,
    setMoreToolsReturnTab,
    storageType,
    setStorageType,
    onboardingStatus,
    setOnboardingStatus,
    mountedTabs,
    setMountedTabs,
    syncStatus,
    setSyncStatus,
    syncError,
    setSyncError,
    networkCircuitOpen,
    setNetworkCircuitOpen,
    sshTunnelSummary,
    setSshTunnelSummary,
    protocolRouterHeaderStatus,
    setProtocolRouterHeaderStatus,
    aiGatewayHeaderStatus,
    setAiGatewayHeaderStatus,
    fileSharingHeaderStatus,
    setFileSharingHeaderStatus,
  };
}
