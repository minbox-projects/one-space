import type {
  LegacySettingsSection,
  ProxyConfig,
  StorageConfig,
} from "../types";
import type {
  ProtocolRouterConfig,
} from "@/lib/protocolRouter";
import type { SslConfig } from "@/lib/sslCertificate";
import type { ServerMonitorSettings } from "@/lib/serverMonitor";
import type { GatewaySettings } from "@/lib/aiGateway";
import {
  getTabSnapshot,
  normalizeProtocolRouterForCompare,
  parseTemplateAutoRefreshInput,
  parseUsageRetentionInput,
} from "../helpers";
import { ALL_LEGACY_SECTIONS } from "./constants";

export interface TabDirtyInput {
  config: StorageConfig;
  proxyConfig: ProxyConfig;
  savedConfig: StorageConfig;
  savedProxyConfig: ProxyConfig;
  protocolRouterConfig: ProtocolRouterConfig;
  savedProtocolRouterConfig: ProtocolRouterConfig;
  aiNewsSyncIntervalInput: string;
  savedAiNewsSyncIntervalInput: string;
  usageRetentionInput: string;
  savedUsageRetentionDays: number;
  templateAutoRefreshInput: string;
  savedTemplateAutoRefreshMinutes: number;
  gatewayPortInput: string;
  savedGatewayPort: number;
  gatewaySettings: GatewaySettings;
  savedGatewaySettings: GatewaySettings;
  sslConfig: SslConfig;
  savedSslConfig: SslConfig;
  sslAlertDaysInput: string;
  serverMonitorSettings: ServerMonitorSettings;
  savedServerMonitorSettings: ServerMonitorSettings;
  serverMonitorIntervalInput: string;
  serverMonitorRetentionInput: string;
}

export function computeTabDirtyMap(input: TabDirtyInput): Record<string, boolean> {
  const {
    config,
    proxyConfig,
    savedConfig,
    savedProxyConfig,
    protocolRouterConfig,
    savedProtocolRouterConfig,
    aiNewsSyncIntervalInput,
    savedAiNewsSyncIntervalInput,
    usageRetentionInput,
    savedUsageRetentionDays,
    templateAutoRefreshInput,
    savedTemplateAutoRefreshMinutes,
    gatewayPortInput,
    savedGatewayPort,
    gatewaySettings,
    savedGatewaySettings,
    sslConfig,
    savedSslConfig,
    sslAlertDaysInput,
    serverMonitorSettings,
    savedServerMonitorSettings,
    serverMonitorIntervalInput,
    serverMonitorRetentionInput,
  } = input;

  const next: Record<string, boolean> = {};

  ALL_LEGACY_SECTIONS.forEach((tab: LegacySettingsSection) => {
    if (tab === "security") {
      next[tab] = false;
      return;
    }
    if (tab === "protocol-router") {
      next[tab] =
        JSON.stringify(normalizeProtocolRouterForCompare(protocolRouterConfig)) !==
        JSON.stringify(normalizeProtocolRouterForCompare(savedProtocolRouterConfig));
      return;
    }
    if (tab === "ai-gateway") {
      const parsed = parseUsageRetentionInput(usageRetentionInput);
      const retentionDirty =
        parsed !== null
          ? parsed !== savedUsageRetentionDays
          : usageRetentionInput.trim() !== String(savedUsageRetentionDays);
      const parsedMinutes = parseTemplateAutoRefreshInput(
        templateAutoRefreshInput,
      );
      const refreshDirty =
        parsedMinutes !== null
          ? parsedMinutes !== savedTemplateAutoRefreshMinutes
          : templateAutoRefreshInput.trim() !==
            String(savedTemplateAutoRefreshMinutes);
      const portDirty = gatewayPortInput.trim() !== String(savedGatewayPort);
      const settingsDirty =
        JSON.stringify(gatewaySettings) !== JSON.stringify(savedGatewaySettings);
      next[tab] = retentionDirty || refreshDirty || portDirty || settingsDirty;
      return;
    }
    if (tab === "ssl-certificate") {
      const thresholdNum = parseInt(sslAlertDaysInput.trim(), 10);
      const savedThreshold = savedSslConfig?.alertDaysThreshold ?? 30;
      const thresholdDirty =
        isNaN(thresholdNum)
          ? sslAlertDaysInput.trim() !== String(savedThreshold)
          : thresholdNum !== savedThreshold;
      const configDirty =
        (sslConfig?.autoCheckEnabled ?? true) !==
          (savedSslConfig?.autoCheckEnabled ?? true) ||
        JSON.stringify(sslConfig?.checkTimes ?? ["09:00"]) !==
          JSON.stringify(savedSslConfig?.checkTimes ?? ["09:00"]);
      next[tab] = thresholdDirty || configDirty;
      return;
    }
    if (tab === "server-monitor") {
      const scheduleDirty =
        serverMonitorSettings.autoCollectEnabled !==
          savedServerMonitorSettings.autoCollectEnabled ||
        serverMonitorSettings.scheduleMode !==
          savedServerMonitorSettings.scheduleMode ||
        JSON.stringify(serverMonitorSettings.dailyTimes) !==
          JSON.stringify(savedServerMonitorSettings.dailyTimes);
      next[tab] =
        scheduleDirty ||
        serverMonitorIntervalInput.trim() !==
          String(savedServerMonitorSettings.intervalMinutes) ||
        serverMonitorRetentionInput.trim() !==
          String(savedServerMonitorSettings.retentionDays);
      return;
    }
    if (tab === "news") {
      const current = getTabSnapshot(tab, config, proxyConfig);
      const saved = getTabSnapshot(tab, savedConfig, savedProxyConfig);
      next[tab] =
        JSON.stringify({
          ...current,
          ai_news_sync_interval_minutes: aiNewsSyncIntervalInput,
        }) !==
        JSON.stringify({
          ...saved,
          ai_news_sync_interval_minutes: savedAiNewsSyncIntervalInput,
        });
      return;
    }
    const current = getTabSnapshot(tab, config, proxyConfig);
    const saved = getTabSnapshot(tab, savedConfig, savedProxyConfig);
    next[tab] = JSON.stringify(current) !== JSON.stringify(saved);
  });

  return next;
}
