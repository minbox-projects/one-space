import { useTranslation } from "react-i18next";
import { ShieldCheck, Server, CloudCog } from "lucide-react";
import { TelegramIcon } from "@/components/common/TelegramIcon";
import { SubTabNavigation, type SubTabItem } from "../common/SubTabNavigation";
import { ServerMonitorSettingsView } from "./ServerMonitorSettings";
import {
  DEFAULT_SERVER_MONITOR_SETTINGS,
  type ServerMonitorSettings,
} from "@/lib/serverMonitor";
import type { LegacySettingsSection } from "../types";
import type { SslConfig } from "@/lib/sslCertificate";
import { SslCertificateSettingsPanel } from "./services/SslCertificateSettingsPanel";
import { CloudAgentSettingsPanel } from "./services/CloudAgentSettingsPanel";
import { RemoteBotSettingsPanel } from "./services/RemoteBotSettingsPanel";
import {
  DEFAULT_CLOUD_AGENT_SETTINGS,
  type CloudAgentSettings,
} from "@/lib/cloudAgent";

export interface OperationsSectionProps {
  subTab: LegacySettingsSection;
  onSubTabChange: (tab: LegacySettingsSection) => void;
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

export function OperationsSection({
  subTab,
  onSubTabChange,
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
}: OperationsSectionProps) {
  const { t } = useTranslation();

  const subTabItems: SubTabItem[] = [
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
      fallbackTitle: "AI 云端智能体",
      icon: CloudCog,
      dirty: subTabDirtyMap["cloud-agent"],
    },
    {
      id: "remote-bot",
      titleKey: "remoteBot",
      fallbackTitle: "Telegram 远程工作台",
      icon: TelegramIcon,
      dirty: subTabDirtyMap["remote-bot"],
    },
  ];

  return (
    <div className="space-y-6">
      <SubTabNavigation
        items={subTabItems}
        activeSubTab={subTab}
        onChange={onSubTabChange}
      />

      {/* 1. SSL Certificate */}
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

      {/* 2. Server Monitor */}
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

      {/* 3. Cloud Agent */}
      {subTab === "cloud-agent" && (
        <CloudAgentSettingsPanel
          t={t}
          settings={cloudAgentSettings}
          setSettings={setCloudAgentSettings || (() => {})}
          onShowToast={onShowToast}
        />
      )}

      {/* 4. Remote Bot (Telegram) */}
      {subTab === "remote-bot" && (
        <RemoteBotSettingsPanel t={t} onShowToast={onShowToast} />
      )}
    </div>
  );
}
