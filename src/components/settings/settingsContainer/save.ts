import type { Dispatch, SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import type { TFunction } from "i18next";
import { errorToMessage } from "@/lib/messages";
import { runUserAction, type ActionContext } from "@/lib/userActions";
import {
  protocolRouterGetConfig,
  protocolRouterSaveConfig,
  protocolRouterStatus,
  type ProtocolRouterConfig,
  type ProtocolRouterStatus,
} from "@/lib/protocolRouter";
import {
  aiGatewaySaveSettings,
  aiGatewayTemplateAutoRefreshGet,
  aiGatewayTemplateAutoRefreshSave,
  aiGatewayUsageRetentionSave,
  notifyTemplateAutoRefreshIntervalChanged,
  type GatewaySettings,
} from "@/lib/aiGateway";
import { sslSaveConfig, type SslConfig } from "@/lib/sslCertificate";
import {
  normalizeServerMonitorSettings,
  serverMonitorSaveSettings,
  validateServerMonitorSettings,
  type ServerMonitorSettings,
} from "@/lib/serverMonitor";
import type {
  LegacySettingsSection,
  ProxyConfig,
  StorageConfig,
} from "../types";
import {
  normalizeConfigForUi,
  normalizeProtocolRouterConfigForUi,
  normalizeProxyConfigForUi,
  parseAiNewsSyncIntervalInput,
  parseTemplateAutoRefreshInput,
  parseUsageRetentionInput,
} from "../helpers";
import { ALL_LEGACY_SECTIONS } from "./constants";
import { parseBoundedInteger } from "./helpers";
import { buildPayloadForTab } from "./payload";
import {
  syncDraftWithLatestForTab,
  type DraftSyncSetters,
} from "./draftSync";
import type { SettingsMessage } from "./types";

export interface SettingsSaveContext {
  t: TFunction;
  activeSubTab: LegacySettingsSection;
  actionContext: ActionContext;
  tabDirtyMap: Record<string, boolean>;
  config: StorageConfig;
  proxyConfig: ProxyConfig;
  aiNewsSyncIntervalInput: string;
  setLoading: Dispatch<SetStateAction<boolean>>;
  setMessage: Dispatch<SetStateAction<SettingsMessage>>;
  getAutostartEnabled: () => Promise<boolean | null>;
  setAutostartEnabled: (enabled: boolean) => Promise<void>;
  draftSync: DraftSyncSetters;
  setSavedConfig: Dispatch<SetStateAction<StorageConfig>>;
  setSavedProxyConfig: Dispatch<SetStateAction<ProxyConfig>>;
  protocolRouterConfig: ProtocolRouterConfig;
  setProtocolRouterConfig: Dispatch<SetStateAction<ProtocolRouterConfig>>;
  setSavedProtocolRouterConfig: Dispatch<SetStateAction<ProtocolRouterConfig>>;
  setProtocolRouterStatusState: Dispatch<
    SetStateAction<ProtocolRouterStatus | null>
  >;
  gatewayPortInput: string;
  setGatewayPortInput: Dispatch<SetStateAction<string>>;
  gatewaySettings: GatewaySettings;
  setGatewaySettings: Dispatch<SetStateAction<GatewaySettings>>;
  setSavedGatewayPort: Dispatch<SetStateAction<number>>;
  setSavedGatewaySettings: Dispatch<SetStateAction<GatewaySettings>>;
  usageRetentionInput: string;
  setUsageRetentionInput: Dispatch<SetStateAction<string>>;
  setSavedUsageRetentionDays: Dispatch<SetStateAction<number>>;
  templateAutoRefreshInput: string;
  setTemplateAutoRefreshInput: Dispatch<SetStateAction<string>>;
  setSavedTemplateAutoRefreshMinutes: Dispatch<SetStateAction<number>>;
  sslConfig: SslConfig;
  sslAlertDaysInput: string;
  setSslConfig: Dispatch<SetStateAction<SslConfig>>;
  setSavedSslConfig: Dispatch<SetStateAction<SslConfig>>;
  setSslAlertDaysInput: Dispatch<SetStateAction<string>>;
  serverMonitorSettings: ServerMonitorSettings;
  serverMonitorIntervalInput: string;
  serverMonitorRetentionInput: string;
  setServerMonitorSettings: Dispatch<SetStateAction<ServerMonitorSettings>>;
  setSavedServerMonitorSettings: Dispatch<
    SetStateAction<ServerMonitorSettings>
  >;
  setServerMonitorIntervalInput: Dispatch<SetStateAction<string>>;
  setServerMonitorRetentionInput: Dispatch<SetStateAction<string>>;
}

export function createSaveCurrentTabHandler({
  t,
  activeSubTab,
  actionContext,
  tabDirtyMap,
  config,
  proxyConfig,
  aiNewsSyncIntervalInput,
  setLoading,
  setMessage,
  getAutostartEnabled,
  setAutostartEnabled,
  draftSync,
  setSavedConfig,
  setSavedProxyConfig,
  protocolRouterConfig,
  setProtocolRouterConfig,
  setSavedProtocolRouterConfig,
  setProtocolRouterStatusState,
  gatewayPortInput,
  setGatewayPortInput,
  gatewaySettings,
  setGatewaySettings,
  setSavedGatewayPort,
  setSavedGatewaySettings,
  usageRetentionInput,
  setUsageRetentionInput,
  setSavedUsageRetentionDays,
  templateAutoRefreshInput,
  setTemplateAutoRefreshInput,
  setSavedTemplateAutoRefreshMinutes,
  sslConfig,
  sslAlertDaysInput,
  setSslConfig,
  setSavedSslConfig,
  setSslAlertDaysInput,
  serverMonitorSettings,
  serverMonitorIntervalInput,
  serverMonitorRetentionInput,
  setServerMonitorSettings,
  setSavedServerMonitorSettings,
  setServerMonitorIntervalInput,
  setServerMonitorRetentionInput,
}: SettingsSaveContext) {
  const handleSaveCurrentTab = async () => {
    if (activeSubTab === "security") return;
    setLoading(true);
    setMessage({ type: "", text: "" });

    try {
      if (activeSubTab === "protocol-router") {
        const saved = await runUserAction(
          actionContext,
          {
            source: "settings",
            category: "protocol_router",
            action: "save-protocol-router",
            target: { tab: "settings", section: "protocol-router" },
            dedupeKey: "settings:protocol-router:save",
            confirm: {
              message: t(
                "confirmSaveProtocolRouter",
                "Save Protocol Router settings and apply the new runtime state now?",
              ),
              title: t("confirmSaveProtocolRouterTitle", "Save Protocol Router settings"),
              okLabel: t("save", "Save"),
              cancelLabel: t("cancel", "Cancel"),
              kind: "warning",
            },
            success: {
              title: t("settingsSectionSavedTitle", "Settings saved"),
              summary: t("currentSectionSavedSuccess", "Current section saved."),
            },
            error: {
              title: t("settingsSaveFailedTitle", "Failed to save settings"),
            },
          },
          async () => {
            const latest = normalizeProtocolRouterConfigForUi(
              await protocolRouterGetConfig(),
            );
            return protocolRouterSaveConfig({
              ...latest,
              port: protocolRouterConfig.port,
              token: protocolRouterConfig.token,
              retention_days: protocolRouterConfig.retention_days,
            });
          },
        );
        if (saved === null) return;
        const normalized = normalizeProtocolRouterConfigForUi(saved);
        setProtocolRouterConfig(normalized);
        setSavedProtocolRouterConfig(normalized);
        setProtocolRouterStatusState(await protocolRouterStatus());
        setMessage({
          type: "success",
          text: t("currentSectionSavedSuccess", "Current section saved."),
        });
        setTimeout(() => setMessage({ type: "", text: "" }), 3000);
        return;
      }

      if (activeSubTab === "ai-gateway") {
        const portNum = parseInt(gatewayPortInput.trim(), 10);
        if (isNaN(portNum) || portNum < 1024 || portNum > 65535) {
          setMessage({
            type: "error",
            text: t(
              "aiGatewayPortInvalid",
              "Port must be an integer between 1024 and 65535.",
            ),
          });
          return;
        }

        try {
          const updatedConfig = await aiGatewaySaveSettings(gatewaySettings, portNum);
          if (updatedConfig) {
            const newPort = updatedConfig.port ?? portNum;
            setSavedGatewayPort(newPort);
            setGatewayPortInput(String(newPort));
            const newSettings = updatedConfig.settings ?? gatewaySettings;
            setSavedGatewaySettings(newSettings);
            setGatewaySettings(newSettings);
          }
        } catch (err: any) {
          setMessage({
            type: "error",
            text: `${t("aiGatewaySaveSettingsFailed", "Failed to save AI Gateway settings")}: ${err?.message || err}`,
          });
          return;
        }

        const parsed = parseUsageRetentionInput(usageRetentionInput);
        if (parsed === null) {
          setMessage({
            type: "error",
            text: t(
              "aiGatewayRetentionInvalid",
              "Retention days must be a whole number between 1 and 365.",
            ),
          });
          return;
        }
        const parsedMinutes = parseTemplateAutoRefreshInput(
          templateAutoRefreshInput,
        );
        if (parsedMinutes === null) {
          setMessage({
            type: "error",
            text: t(
              "aiGatewayTemplateAutoRefreshInvalid",
              "Template auto refresh minutes must be 0 (disabled) or between 10 and 1440",
            ),
          });
          return;
        }
        const saved = await aiGatewayUsageRetentionSave(parsed);
        const normalized = Number.isFinite(saved) ? Number(saved) : parsed;
        setSavedUsageRetentionDays(normalized);
        setUsageRetentionInput(String(normalized));
        let savedMinutes: number;
        try {
          savedMinutes = await aiGatewayTemplateAutoRefreshSave(parsedMinutes);
        } catch {
          setMessage({
            type: "error",
            text: `${t(
              "aiGatewaySettingsPartialSaved",
              "Log retention saved, but template auto-refresh was not updated.",
            )} ${t(
              "aiGatewayTemplateAutoRefreshInvalid",
              "Template auto refresh minutes must be 0 (disabled) or between 10 and 1440",
            )}`,
          });
          return;
        }
        const normalizedMinutes = Number.isFinite(savedMinutes)
          ? Number(savedMinutes)
          : parsedMinutes;
        setSavedTemplateAutoRefreshMinutes(normalizedMinutes);
        setTemplateAutoRefreshInput(String(normalizedMinutes));
        try {
          const persisted = await aiGatewayTemplateAutoRefreshGet();
          if (Number.isFinite(persisted)) {
            const reReadMinutes = Number(persisted);
            setSavedTemplateAutoRefreshMinutes(reReadMinutes);
            setTemplateAutoRefreshInput(String(reReadMinutes));
          }
        } catch {}
        notifyTemplateAutoRefreshIntervalChanged();
        setMessage({
          type: "success",
          text: t("aiGatewaySettingsSaved", "AI Gateway settings saved."),
        });
        setTimeout(() => setMessage({ type: "", text: "" }), 3000);
        return;
      }

      if (activeSubTab === "ssl-certificate") {
        const parsedThreshold = parseInt(sslAlertDaysInput.trim(), 10);
        if (
          isNaN(parsedThreshold) ||
          parsedThreshold < 1 ||
          parsedThreshold > 365
        ) {
          setMessage({
            type: "error",
            text: t(
              "sslAlertThresholdInvalid",
              "告警阈值必须是 1 到 365 之间的整数",
            ),
          });
          return;
        }
        const payloadToSave: SslConfig = {
          ...sslConfig,
          alertDaysThreshold: parsedThreshold,
        };
        const saved = await runUserAction(
          actionContext,
          {
            source: "settings",
            category: "ssl-certificate",
            action: "save-ssl-config",
            target: { tab: "settings", section: "ssl-certificate" },
            dedupeKey: "settings:ssl-certificate:save",
            confirm: {
              message: t(
                "confirmSaveSslConfig",
                "Save changes to SSL Certificate settings?",
              ),
              title: t("confirmSaveSslConfigTitle", "Save SSL Certificate settings"),
              okLabel: t("save", "Save"),
              cancelLabel: t("cancel", "Cancel"),
              kind: "warning",
            },
            success: {
              title: t("settingsSectionSavedTitle", "Settings saved"),
              summary: t("currentSectionSavedSuccess", "Current section saved."),
            },
            error: {
              title: t("settingsSaveFailedTitle", "Failed to save settings"),
            },
          },
          async () => sslSaveConfig(payloadToSave),
        );
        if (saved === null) return;
        setSslConfig(saved);
        setSavedSslConfig(saved);
        setSslAlertDaysInput(String(saved.alertDaysThreshold));
        setMessage({
          type: "success",
          text: t("currentSectionSavedSuccess", "Current section saved."),
        });
        setTimeout(() => setMessage({ type: "", text: "" }), 3000);
        return;
      }

      if (activeSubTab === "server-monitor") {
        const intervalMinutes = parseBoundedInteger(
          serverMonitorIntervalInput,
          1,
          1440,
        );
        if (intervalMinutes === null) {
          setMessage({
            type: "error",
            text: t(
              "serverMonitorIntervalInvalid",
              "Interval must be an integer from 1 to 1440 minutes.",
            ),
          });
          return;
        }
        const retentionDays = parseBoundedInteger(
          serverMonitorRetentionInput,
          1,
          30,
        );
        if (retentionDays === null) {
          setMessage({
            type: "error",
            text: t(
              "serverMonitorRetentionInvalid",
              "Retention must be an integer from 1 to 30 days.",
            ),
          });
          return;
        }
        const candidate: ServerMonitorSettings = {
          ...serverMonitorSettings,
          intervalMinutes,
          retentionDays,
        };
        if (!validateServerMonitorSettings(candidate)) {
          setMessage({
            type: "error",
            text: t(
              "serverMonitorInvalidSettings",
              "Check the interval, retention, and daily collection times.",
            ),
          });
          return;
        }
        const payloadToSave: ServerMonitorSettings = {
          ...candidate,
          dailyTimes: [...candidate.dailyTimes].sort(),
        };
        const saved = await runUserAction(
          actionContext,
          {
            source: "settings",
            category: "server-monitor",
            action: "save-server-monitor-settings",
            target: { tab: "settings", section: "server-monitor" },
            dedupeKey: "settings:server-monitor:save",
            confirm: {
              message: t(
                "confirmSaveServerMonitor",
                "Save Server Monitor scheduling settings?",
              ),
              title: t("serverMonitorScheduleTitle", "Collection schedule"),
              okLabel: t("save", "Save"),
              cancelLabel: t("cancel", "Cancel"),
              kind: "warning",
            },
            success: {
              title: t("settingsSectionSavedTitle", "Settings saved"),
              summary: t("currentSectionSavedSuccess", "Current section saved."),
            },
            error: {
              title: t("settingsSaveFailedTitle", "Failed to save settings"),
            },
          },
          async () => serverMonitorSaveSettings(payloadToSave),
        );
        if (saved === null) return;
        const normalized = normalizeServerMonitorSettings(saved.settings ?? payloadToSave);
        setServerMonitorSettings(normalized);
        setSavedServerMonitorSettings(normalized);
        setServerMonitorIntervalInput(String(normalized.intervalMinutes));
        setServerMonitorRetentionInput(String(normalized.retentionDays));
        setMessage({
          type: "success",
          text: t("currentSectionSavedSuccess", "Current section saved."),
        });
        setTimeout(() => setMessage({ type: "", text: "" }), 3000);
        return;
      }

      const otherTabsDirtyBeforeSave = ALL_LEGACY_SECTIONS.some(
        (tab) => tab !== activeSubTab && tabDirtyMap[tab],
      );
      const baseRaw = await invoke<StorageConfig>("get_storage_config");
      const baseConfig = normalizeConfigForUi(
        baseRaw,
        t("aiTerminalAppPlaceholder", "终端"),
      );

      if (activeSubTab === "news") {
        const parsedInterval = parseAiNewsSyncIntervalInput(
          aiNewsSyncIntervalInput,
        );
        if (parsedInterval === null) {
          setMessage({
            type: "error",
            text: t(
              "newsSyncIntervalValidation",
              "Fetch interval must be an integer between 10 and 1440 minutes.",
            ),
          });
          return;
        }
      }

      const draftConfig =
        activeSubTab === "news"
          ? {
              ...config,
              ai_news_sync_interval_minutes: parseAiNewsSyncIntervalInput(
                aiNewsSyncIntervalInput,
              )!,
            }
          : config;

      const payload = buildPayloadForTab(
        activeSubTab,
        draftConfig,
        proxyConfig,
        baseConfig,
      );

      const saveResult = await runUserAction(
        actionContext,
        {
          source: "settings",
          category: "save",
          action: `save-${activeSubTab}`,
          target: { tab: "settings", section: activeSubTab },
          dedupeKey: `settings:save:${activeSubTab}`,
          metadata: { tab: activeSubTab },
          confirm: {
            message: t(
              "confirmSaveSettingsSection",
              "Save changes in this settings section now?",
            ),
            title: t("confirmSaveSettingsSectionTitle", "Save settings"),
            okLabel: t("save", "Save"),
            cancelLabel: t("cancel", "Cancel"),
            kind: "warning",
          },
          success: {
            title: t("settingsSectionSavedTitle", "Settings saved"),
            summary: t("currentSectionSavedSuccess", "Current section saved."),
          },
          error: {
            title: t("settingsSaveFailedTitle", "Failed to save settings"),
          },
        },
        async () => {
          await invoke("save_storage_config", { config: payload });
          return { ok: true };
        },
      );
      if (saveResult === null) return;

      if (activeSubTab === "general") {
        const desiredAutostart = !!payload.launch_at_login;
        const currentAutostart = await getAutostartEnabled();
        if (
          currentAutostart === null ||
          currentAutostart !== desiredAutostart
        ) {
          await setAutostartEnabled(desiredAutostart);
        }
      }

      if (activeSubTab === "shortcuts") {
        await invoke("update_shortcuts", {
          main: config.main_shortcut,
          quick: config.quick_ai_shortcut,
        });
        await emit("tray-shortcuts-updated", {
          main: config.main_shortcut,
          quick: config.quick_ai_shortcut,
        }).catch(console.error);
      }

      if (activeSubTab === "ai") {
        await emit("refresh-counts").catch(console.error);
      }

      const latestRaw = await invoke<StorageConfig>("get_storage_config");
      const latestAutostart = await getAutostartEnabled();
      const latestConfig = normalizeConfigForUi(
        {
          ...latestRaw,
          launch_at_login:
            latestAutostart ?? latestRaw.launch_at_login ?? false,
        },
        t("aiTerminalAppPlaceholder", "终端"),
      );
      const latestProxy = normalizeProxyConfigForUi(latestRaw.proxy);
      setSavedConfig(latestConfig);
      setSavedProxyConfig(latestProxy);
      syncDraftWithLatestForTab(activeSubTab, latestConfig, latestProxy, draftSync);

      const baseText = t(
        "currentSectionSavedSuccess",
        "Current section saved.",
      );
      setMessage({
        type: "success",
        text: otherTabsDirtyBeforeSave
          ? `${baseText} ${t("otherSectionsUnsaved", "Other sections still have unsaved changes.")}`
          : baseText,
      });
      setTimeout(() => setMessage({ type: "", text: "" }), 3000);
    } catch (e: any) {
      setMessage({ type: "error", text: errorToMessage(e) });
    } finally {
      setLoading(false);
    }
  };

  return handleSaveCurrentTab;
}
