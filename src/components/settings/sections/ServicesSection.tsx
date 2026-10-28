import { useTranslation } from "react-i18next";
import {
  Globe,
  Route,
  Network,
  ShieldCheck,
  Server,
  CloudCog,
} from "lucide-react";
import { SubTabNavigation, type SubTabItem } from "../common/SubTabNavigation";
import { ServerMonitorSettingsView } from "./ServerMonitorSettings";
import {
  DEFAULT_SERVER_MONITOR_SETTINGS,
  type ServerMonitorSettings,
} from "@/lib/serverMonitor";
import type {
  ProxyConfig,
  ProxyStatus,
  LegacySettingsSection,
} from "../types";
import type {
  ProtocolRouterConfig,
  ProtocolRouterStatus,
} from "@/lib/protocolRouter";
import type { SslConfig } from "@/lib/sslCertificate";
import {
  type GatewaySettings,
  DEFAULT_GATEWAY_SETTINGS,
} from "@/lib/aiGateway";
import { ProxySettingsPanel } from "./services/ProxySettingsPanel";
import { ProtocolRouterSettingsPanel } from "./services/ProtocolRouterSettingsPanel";
import { AiGatewaySettingsPanel } from "./services/AiGatewaySettingsPanel";
import { SslCertificateSettingsPanel } from "./services/SslCertificateSettingsPanel";
import { CloudAgentSettingsPanel } from "./services/CloudAgentSettingsPanel";
import {
  DEFAULT_CLOUD_AGENT_SETTINGS,
  type CloudAgentSettings,
} from "@/lib/cloudAgent";

export interface ServicesSectionProps {
  subTab: LegacySettingsSection;
  onSubTabChange: (tab: LegacySettingsSection) => void;
  proxyConfig: ProxyConfig;
  setProxyConfig: React.Dispatch<React.SetStateAction<ProxyConfig>>;
  proxyStatus: ProxyStatus | null;
  testingProxy?: boolean;
  onTestProxy?: () => Promise<void>;
  authEnabled: boolean;
  setAuthEnabled: (enabled: boolean) => void;
  protocolRouterConfig: ProtocolRouterConfig;
  setProtocolRouterConfig: React.Dispatch<React.SetStateAction<ProtocolRouterConfig>>;
  protocolRouterStatusState: ProtocolRouterStatus | null;
  protocolRouterBusy?: boolean;
  onRotateProtocolToken?: () => Promise<void>;
  usageRetentionInput: string;
  setUsageRetentionInput: (val: string) => void;
  templateAutoRefreshInput: string;
  setTemplateAutoRefreshInput: (val: string) => void;
  gatewaySettings?: GatewaySettings;
  setGatewaySettings?: React.Dispatch<React.SetStateAction<GatewaySettings>>;
  gatewayPortInput?: string;
  setGatewayPortInput?: (val: string) => void;
  sslConfig: SslConfig;
  setSslConfig: React.Dispatch<React.SetStateAction<SslConfig>>;
  newCheckTimeInput: string;
  setNewCheckTimeInput: (val: string) => void;
  sslAlertDaysInput: string;
  setSslAlertDaysInput: (val: string) => void;
  serverMonitorSettings?: ServerMonitorSettings;
  setServerMonitorSettings?: React.Dispatch<React.SetStateAction<ServerMonitorSettings>>;
  serverMonitorIntervalInput?: string;
  setServerMonitorIntervalInput?: (val: string) => void;
  serverMonitorRetentionInput?: string;
  setServerMonitorRetentionInput?: (val: string) => void;
  serverMonitorNewTimeInput?: string;
  setServerMonitorNewTimeInput?: (val: string) => void;
  cloudAgentSettings?: CloudAgentSettings;
  setCloudAgentSettings?: React.Dispatch<React.SetStateAction<CloudAgentSettings>>;
  onShowToast?: (type: "success" | "error", text: string) => void;
  subTabDirtyMap?: Record<string, boolean>;
}

export function ServicesSection({
  subTab,
  onSubTabChange,
  proxyConfig,
  setProxyConfig,
  proxyStatus,
  testingProxy = false,
  onTestProxy,
  authEnabled,
  setAuthEnabled,
  protocolRouterConfig,
  setProtocolRouterConfig,
  protocolRouterStatusState,
  protocolRouterBusy = false,
  onRotateProtocolToken,
  usageRetentionInput,
  setUsageRetentionInput,
  templateAutoRefreshInput,
  setTemplateAutoRefreshInput,
  gatewaySettings = DEFAULT_GATEWAY_SETTINGS,
  setGatewaySettings,
  gatewayPortInput = "17688",
  setGatewayPortInput,
  sslConfig,
  setSslConfig,
  newCheckTimeInput,
  setNewCheckTimeInput,
  sslAlertDaysInput,
  setSslAlertDaysInput,
  serverMonitorSettings = DEFAULT_SERVER_MONITOR_SETTINGS,
  setServerMonitorSettings,
  serverMonitorIntervalInput = "5",
  setServerMonitorIntervalInput,
  serverMonitorRetentionInput = "7",
  setServerMonitorRetentionInput,
  serverMonitorNewTimeInput = "",
  setServerMonitorNewTimeInput,
  cloudAgentSettings = DEFAULT_CLOUD_AGENT_SETTINGS,
  setCloudAgentSettings,
  onShowToast,
  subTabDirtyMap = {},
}: ServicesSectionProps) {
  const { t } = useTranslation();

  const subTabItems: SubTabItem[] = [
    {
      id: "proxy",
      titleKey: "proxy",
      fallbackTitle: "网络代理",
      icon: Globe,
      dirty: subTabDirtyMap["proxy"],
    },
    {
      id: "protocol-router",
      titleKey: "protocolRouter",
      fallbackTitle: "协议路由",
      icon: Route,
      dirty: subTabDirtyMap["protocol-router"],
    },
    {
      id: "ai-gateway",
      titleKey: "aiGatewaySettings",
      fallbackTitle: "AI 网关",
      icon: Network,
      dirty: subTabDirtyMap["ai-gateway"],
    },
    {
      id: "ssl-certificate",
      titleKey: "sslCertificate",
      fallbackTitle: "SSL 证书",
      icon: ShieldCheck,
      dirty: subTabDirtyMap["ssl-certificate"],
    },
    {
      id: "server-monitor",
      titleKey: "serverMonitor",
      fallbackTitle: "服务器监控",
      icon: Server,
      dirty: subTabDirtyMap["server-monitor"],
    },
    {
      id: "cloud-agent",
      titleKey: "cloudAgent",
      fallbackTitle: "云端智能体",
      icon: CloudCog,
      dirty: subTabDirtyMap["cloud-agent"],
    },
  ];

  return (
    <div className="space-y-6">
      <SubTabNavigation
        items={subTabItems}
        activeSubTab={subTab}
        onChange={onSubTabChange}
      />

      {/* 1. Network Proxy */}
      {subTab === "proxy" && (
        <ProxySettingsPanel
          t={t}
          proxyConfig={proxyConfig}
          setProxyConfig={setProxyConfig}
          proxyStatus={proxyStatus}
          testingProxy={testingProxy}
          onTestProxy={onTestProxy}
          authEnabled={authEnabled}
          setAuthEnabled={setAuthEnabled}
        />
      )}

      {/* 2. Protocol Router */}
      {subTab === "protocol-router" && (
        <ProtocolRouterSettingsPanel
          t={t}
          protocolRouterConfig={protocolRouterConfig}
          setProtocolRouterConfig={setProtocolRouterConfig}
          protocolRouterStatusState={protocolRouterStatusState}
          protocolRouterBusy={protocolRouterBusy}
          onRotateProtocolToken={onRotateProtocolToken}
          onShowToast={onShowToast}
        />
      )}

      {/* 3. AI Gateway */}
      {subTab === "ai-gateway" && (
        <AiGatewaySettingsPanel
          t={t}
          gatewaySettings={gatewaySettings}
          setGatewaySettings={setGatewaySettings}
          gatewayPortInput={gatewayPortInput}
          setGatewayPortInput={setGatewayPortInput}
          usageRetentionInput={usageRetentionInput}
          setUsageRetentionInput={setUsageRetentionInput}
          templateAutoRefreshInput={templateAutoRefreshInput}
          setTemplateAutoRefreshInput={setTemplateAutoRefreshInput}
        />
      )}

      {/* 4. SSL Certificate */}
      {subTab === "ssl-certificate" && (
        <SslCertificateSettingsPanel
          t={t}
          sslConfig={sslConfig}
          setSslConfig={setSslConfig}
          newCheckTimeInput={newCheckTimeInput}
          setNewCheckTimeInput={setNewCheckTimeInput}
          sslAlertDaysInput={sslAlertDaysInput}
          setSslAlertDaysInput={setSslAlertDaysInput}
          onShowToast={onShowToast}
        />
      )}

      {/* 5. Server Monitor */}
      {subTab === "server-monitor" && (
        <ServerMonitorSettingsView
          settings={serverMonitorSettings}
          onSettingsChange={(next) => setServerMonitorSettings?.(next)}
          intervalInput={serverMonitorIntervalInput}
          onIntervalInputChange={(value) => setServerMonitorIntervalInput?.(value)}
          retentionInput={serverMonitorRetentionInput}
          onRetentionInputChange={(value) => setServerMonitorRetentionInput?.(value)}
          newTimeInput={serverMonitorNewTimeInput}
          onNewTimeInputChange={(value) => setServerMonitorNewTimeInput?.(value)}
          onShowToast={onShowToast}
        />
      )}

      {/* 6. Cloud Agent */}
      {subTab === "cloud-agent" && (
        <CloudAgentSettingsPanel
          t={t}
          settings={cloudAgentSettings}
          setSettings={setCloudAgentSettings || (() => {})}
          onShowToast={onShowToast}
        />
      )}
    </div>
  );
}
