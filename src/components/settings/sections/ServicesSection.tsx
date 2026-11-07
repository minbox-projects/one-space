import { useTranslation } from "react-i18next";
import { Globe, Route, Network } from "lucide-react";
import { SubTabNavigation, type SubTabItem } from "../common/SubTabNavigation";
import type {
  ProxyConfig,
  ProxyStatus,
  LegacySettingsSection,
} from "../types";
import type {
  ProtocolRouterConfig,
  ProtocolRouterStatus,
} from "@/lib/protocolRouter";
import {
  type GatewaySettings,
  DEFAULT_GATEWAY_SETTINGS,
} from "@/lib/aiGateway";
import { ProxySettingsPanel } from "./services/ProxySettingsPanel";
import { ProtocolRouterSettingsPanel } from "./services/ProtocolRouterSettingsPanel";
import { AiGatewaySettingsPanel } from "./services/AiGatewaySettingsPanel";

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
    </div>
  );
}
