import type { Dispatch, SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";
import { errorToMessage } from "@/lib/messages";
import {
  protocolRouterGetConfig,
  protocolRouterStatus,
  type ProtocolRouterConfig,
  type ProtocolRouterStatus,
} from "@/lib/protocolRouter";
import {
  aiGatewayGetConfig,
  aiGatewayTemplateAutoRefreshGet,
  aiGatewayUsageRetentionGet,
  normalizeGatewaySettings,
  type GatewaySettings,
} from "@/lib/aiGateway";
import { sslGetConfig, type SslConfig } from "@/lib/sslCertificate";
import {
  DEFAULT_SERVER_MONITOR_SETTINGS,
  normalizeServerMonitorSettings,
  serverMonitorGetSettings,
  type ServerMonitorSettings,
} from "@/lib/serverMonitor";
import {
  cloudAgentGetSettings,
  DEFAULT_CLOUD_AGENT_SETTINGS,
  type CloudAgentSettings,
} from "@/lib/cloudAgent";
import type {
  ProxyConfig,
  StorageConfig,
} from "../types";
import {
  detectNewsRetentionPreset,
  formatAiNewsSyncIntervalInput,
  normalizeConfigForUi,
  normalizeProxyConfigForUi,
  normalizeProtocolRouterConfigForUi,
  type NewsRetentionPreset,
} from "../helpers";
import type { SettingsMessage } from "./types";

export interface SettingsLoadersContext {
  t: TFunction;
  setConfig: Dispatch<SetStateAction<StorageConfig>>;
  setSavedConfig: Dispatch<SetStateAction<StorageConfig>>;
  setMessage: Dispatch<SetStateAction<SettingsMessage>>;
  setAiNewsSyncIntervalInput: Dispatch<SetStateAction<string>>;
  setSavedAiNewsSyncIntervalInput: Dispatch<SetStateAction<string>>;
  setNewsRetentionPreset: Dispatch<SetStateAction<NewsRetentionPreset>>;
  setProxyConfig: Dispatch<SetStateAction<ProxyConfig>>;
  setSavedProxyConfig: Dispatch<SetStateAction<ProxyConfig>>;
  setAuthEnabled: Dispatch<SetStateAction<boolean>>;
  setMasterPassword: Dispatch<SetStateAction<string>>;
  setProtocolRouterConfig: Dispatch<SetStateAction<ProtocolRouterConfig>>;
  setSavedProtocolRouterConfig: Dispatch<SetStateAction<ProtocolRouterConfig>>;
  setProtocolRouterStatusState: Dispatch<
    SetStateAction<ProtocolRouterStatus | null>
  >;
  setSavedGatewayPort: Dispatch<SetStateAction<number>>;
  setGatewayPortInput: Dispatch<SetStateAction<string>>;
  setSavedGatewaySettings: Dispatch<SetStateAction<GatewaySettings>>;
  setGatewaySettings: Dispatch<SetStateAction<GatewaySettings>>;
  setSavedUsageRetentionDays: Dispatch<SetStateAction<number>>;
  setUsageRetentionInput: Dispatch<SetStateAction<string>>;
  setSavedTemplateAutoRefreshMinutes: Dispatch<SetStateAction<number>>;
  setTemplateAutoRefreshInput: Dispatch<SetStateAction<string>>;
  setSslConfig: Dispatch<SetStateAction<SslConfig>>;
  setSavedSslConfig: Dispatch<SetStateAction<SslConfig>>;
  setSslAlertDaysInput: Dispatch<SetStateAction<string>>;
  setServerMonitorSettings: Dispatch<SetStateAction<ServerMonitorSettings>>;
  setSavedServerMonitorSettings: Dispatch<
    SetStateAction<ServerMonitorSettings>
  >;
  setServerMonitorIntervalInput: Dispatch<SetStateAction<string>>;
  setServerMonitorRetentionInput: Dispatch<SetStateAction<string>>;
  setServerMonitorNewTimeInput: Dispatch<SetStateAction<string>>;
  setCloudAgentSettings?: Dispatch<SetStateAction<CloudAgentSettings>>;
  setSavedCloudAgentSettings?: Dispatch<SetStateAction<CloudAgentSettings>>;
}

export function createSettingsLoaders({
  t,
  setConfig,
  setSavedConfig,
  setMessage,
  setAiNewsSyncIntervalInput,
  setSavedAiNewsSyncIntervalInput,
  setNewsRetentionPreset,
  setProxyConfig,
  setSavedProxyConfig,
  setAuthEnabled,
  setMasterPassword,
  setProtocolRouterConfig,
  setSavedProtocolRouterConfig,
  setProtocolRouterStatusState,
  setSavedGatewayPort,
  setGatewayPortInput,
  setSavedGatewaySettings,
  setGatewaySettings,
  setSavedUsageRetentionDays,
  setUsageRetentionInput,
  setSavedTemplateAutoRefreshMinutes,
  setTemplateAutoRefreshInput,
  setSslConfig,
  setSavedSslConfig,
  setSslAlertDaysInput,
  setServerMonitorSettings,
  setSavedServerMonitorSettings,
  setServerMonitorIntervalInput,
  setServerMonitorRetentionInput,
  setServerMonitorNewTimeInput,
  setCloudAgentSettings,
  setSavedCloudAgentSettings,
}: SettingsLoadersContext) {
  const getAutostartEnabled = async (): Promise<boolean | null> => {
    try {
      return await invoke<boolean>("plugin:autostart|is_enabled");
    } catch (e) {
      console.error(e);
      return null;
    }
  };

  const setAutostartEnabled = async (enabled: boolean) => {
    try {
      if (enabled) {
        await invoke("plugin:autostart|enable");
      } else {
        await invoke("plugin:autostart|disable");
      }
    } catch (e) {
      console.error(e);
    }
  };

  const loadConfig = async () => {
    try {
      const cfg = await invoke<StorageConfig>("get_storage_config");
      const autostartEnabled = await getAutostartEnabled();
      const normalized = normalizeConfigForUi(
        {
          ...cfg,
          launch_at_login: autostartEnabled ?? cfg.launch_at_login ?? false,
        },
        t("aiTerminalAppPlaceholder", "终端"),
      );
      const normalizedProxy = normalizeProxyConfigForUi(cfg.proxy);
      setConfig(normalized);
      setSavedConfig(normalized);
      setAiNewsSyncIntervalInput(
        formatAiNewsSyncIntervalInput(normalized.ai_news_sync_interval_minutes),
      );
      setSavedAiNewsSyncIntervalInput(
        formatAiNewsSyncIntervalInput(normalized.ai_news_sync_interval_minutes),
      );
      setNewsRetentionPreset(
        detectNewsRetentionPreset(
          normalized.ai_news_retention_days,
          normalized.ai_news_retention_max_items,
        ),
      );
      setProxyConfig(normalizedProxy);
      setSavedProxyConfig(normalizedProxy);
      setAuthEnabled(
        !!(normalizedProxy.proxy_username || normalizedProxy.proxy_password),
      );
    } catch (e: any) {
      setMessage({ type: "error", text: errorToMessage(e) });
    }
  };

  const loadMasterPassword = async () => {
    try {
      const pass = await invoke<string>("get_master_password");
      setMasterPassword(pass);
    } catch (e) {
      console.error(e);
    }
  };

  const loadProtocolRouter = async () => {
    try {
      const cfg = await protocolRouterGetConfig();
      const normalized = normalizeProtocolRouterConfigForUi(cfg);
      setProtocolRouterConfig(normalized);
      setSavedProtocolRouterConfig(normalized);
      setProtocolRouterStatusState(await protocolRouterStatus());
    } catch (e) {
      console.error(e);
    }
  };

  const loadAiGateway = async () => {
    try {
      const config = await aiGatewayGetConfig();
      if (config) {
        const port = config.port ?? 17688;
        setSavedGatewayPort(port);
        setGatewayPortInput(String(port));
        const settings = normalizeGatewaySettings(config.settings);
        setSavedGatewaySettings(settings);
        setGatewaySettings(settings);
      }
      const days = await aiGatewayUsageRetentionGet();
      const normalizedDays = Number.isFinite(days) ? Number(days) : 30;
      setSavedUsageRetentionDays(normalizedDays);
      setUsageRetentionInput(String(normalizedDays));
      const refreshMinutes = await aiGatewayTemplateAutoRefreshGet();
      const normalizedMinutes = Number.isFinite(refreshMinutes)
        ? Number(refreshMinutes)
        : 60;
      setSavedTemplateAutoRefreshMinutes(normalizedMinutes);
      setTemplateAutoRefreshInput(String(normalizedMinutes));
    } catch (e) {
      console.error(e);
    }
  };

  const loadSslConfig = async () => {
    try {
      const cfg = await sslGetConfig();
      setSslConfig(cfg);
      setSavedSslConfig(cfg);
      setSslAlertDaysInput(String(cfg.alertDaysThreshold));
    } catch (e) {
      console.error(e);
    }
  };

  const loadServerMonitorSettings = async () => {
    try {
      const cfg = await serverMonitorGetSettings();
      const normalized = normalizeServerMonitorSettings(cfg);
      setServerMonitorSettings(normalized);
      setSavedServerMonitorSettings(normalized);
      setServerMonitorIntervalInput(String(normalized.intervalMinutes));
      setServerMonitorRetentionInput(String(normalized.retentionDays));
      setServerMonitorNewTimeInput("");
    } catch (e) {
      console.error(e);
      setServerMonitorSettings(DEFAULT_SERVER_MONITOR_SETTINGS);
      setSavedServerMonitorSettings(DEFAULT_SERVER_MONITOR_SETTINGS);
      setServerMonitorIntervalInput(String(DEFAULT_SERVER_MONITOR_SETTINGS.intervalMinutes));
      setServerMonitorRetentionInput(String(DEFAULT_SERVER_MONITOR_SETTINGS.retentionDays));
    }
  };

  const loadCloudAgentSettings = async () => {
    try {
      const cfg = await cloudAgentGetSettings();
      setCloudAgentSettings?.(cfg);
      setSavedCloudAgentSettings?.(cfg);
    } catch (e) {
      console.error(e);
      setCloudAgentSettings?.(DEFAULT_CLOUD_AGENT_SETTINGS);
      setSavedCloudAgentSettings?.(DEFAULT_CLOUD_AGENT_SETTINGS);
    }
  };

  return {
    getAutostartEnabled,
    setAutostartEnabled,
    loadConfig,
    loadMasterPassword,
    loadProtocolRouter,
    loadAiGateway,
    loadSslConfig,
    loadServerMonitorSettings,
    loadCloudAgentSettings,
  };
}
