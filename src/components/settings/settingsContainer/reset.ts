import type { Dispatch, SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";
import { notifyMessage } from "@/lib/messages";
import type { ActionContext } from "@/lib/userActions";
import type { SslConfig } from "@/lib/sslCertificate";
import type {
  LegacySettingsSection,
  ProxyConfig,
  StorageConfig,
} from "../types";
import {
  normalizeConfigForUi,
  normalizeProxyConfigForUi,
} from "../helpers";
import { ALL_LEGACY_SECTIONS } from "./constants";
import {
  syncDraftWithLatestForTab,
  type DraftSyncSetters,
} from "./draftSync";
import type { SettingsMessage } from "./types";

export interface SettingsResetContext {
  t: TFunction;
  activeSubTab: LegacySettingsSection;
  tabDirtyMap: Record<string, boolean>;
  confirmDialog: ActionContext["confirm"];
  pushToast: ActionContext["pushToast"];
  setLoading: Dispatch<SetStateAction<boolean>>;
  setMessage: Dispatch<SetStateAction<SettingsMessage>>;
  loadProtocolRouter: () => Promise<void>;
  loadAiGateway: () => Promise<void>;
  loadSslConfig: () => Promise<void>;
  loadServerMonitorSettings: () => Promise<void>;
  loadCloudAgentSettings?: () => Promise<void>;
  savedSslConfig: SslConfig;
  setSslAlertDaysInput: Dispatch<SetStateAction<string>>;
  getAutostartEnabled: () => Promise<boolean | null>;
  setSavedConfig: Dispatch<SetStateAction<StorageConfig>>;
  setSavedProxyConfig: Dispatch<SetStateAction<ProxyConfig>>;
  draftSync: DraftSyncSetters;
}

export function createResetCurrentTabHandler({
  t,
  activeSubTab,
  tabDirtyMap,
  confirmDialog,
  pushToast,
  setLoading,
  setMessage,
  loadProtocolRouter,
  loadAiGateway,
  loadSslConfig,
  loadServerMonitorSettings,
  loadCloudAgentSettings,
  savedSslConfig,
  setSslAlertDaysInput,
  getAutostartEnabled,
  setSavedConfig,
  setSavedProxyConfig,
  draftSync,
}: SettingsResetContext) {
  const handleResetCurrentTab = async () => {
    if (activeSubTab === "security") return;
    setLoading(true);
    setMessage({ type: "", text: "" });

    try {
      if (activeSubTab === "protocol-router") {
        const confirmed = await confirmDialog(
          t(
            "confirmResetProtocolRouter",
            "Discard unsaved Protocol Router changes and reload the saved configuration?",
          ),
          {
            title: t("confirmResetProtocolRouterTitle", "Reset Protocol Router settings"),
            okLabel: t("reset", "Reset"),
            cancelLabel: t("cancel", "Cancel"),
            kind: "warning",
          },
        );
        if (!confirmed) return;
        await loadProtocolRouter();
        await notifyMessage.success({
          source: "settings",
          category: "reset",
          title: t("settingsSectionResetTitle", "Settings reset"),
          summary: t(
            "currentSectionResetSuccess",
            "Current section has been reset.",
          ),
          dedupeKey: "settings:reset:protocol-router",
          target: { tab: "settings", section: "protocol-router" },
        });
        pushToast({
          title: t("settingsSectionResetTitle", "Settings reset"),
          description: t(
            "currentSectionResetSuccess",
            "Current section has been reset.",
          ),
          kind: "success",
        });
        setMessage({
          type: "success",
          text: t("currentSectionResetSuccess", "Current section has been reset."),
        });
        setTimeout(() => setMessage({ type: "", text: "" }), 3000);
        return;
      }

      if (activeSubTab === "ai-gateway") {
        const confirmed = await confirmDialog(
          t(
            "confirmResetSettingsSection",
            "Discard unsaved changes in this settings section and reload the saved values?",
          ),
          {
            title: t("confirmResetSettingsSectionTitle", "Reset settings"),
            okLabel: t("reset", "Reset"),
            cancelLabel: t("cancel", "Cancel"),
            kind: "warning",
          },
        );
        if (!confirmed) return;
        await loadAiGateway();
        setMessage({
          type: "success",
          text: t("currentSectionResetSuccess", "Current section has been reset."),
        });
        setTimeout(() => setMessage({ type: "", text: "" }), 3000);
        return;
      }

      if (activeSubTab === "ssl-certificate") {
        const confirmed = await confirmDialog(
          t(
            "confirmResetSettingsSection",
            "Discard unsaved changes in this settings section and reload the saved values?",
          ),
          {
            title: t("confirmResetSettingsSectionTitle", "Reset settings"),
            okLabel: t("reset", "Reset"),
            cancelLabel: t("cancel", "Cancel"),
            kind: "warning",
          },
        );
        if (!confirmed) return;
        await loadSslConfig();
        setSslAlertDaysInput(String(savedSslConfig.alertDaysThreshold));
        setMessage({
          type: "success",
          text: t("currentSectionResetSuccess", "Current section has been reset."),
        });
        setTimeout(() => setMessage({ type: "", text: "" }), 3000);
        return;
      }

      if (activeSubTab === "server-monitor") {
        const confirmed = await confirmDialog(
          t(
            "confirmResetSettingsSection",
            "Discard unsaved changes in this settings section and reload the saved values?",
          ),
          {
            title: t("confirmResetSettingsSectionTitle", "Reset settings"),
            okLabel: t("reset", "Reset"),
            cancelLabel: t("cancel", "Cancel"),
            kind: "warning",
          },
        );
        if (!confirmed) return;
        await loadServerMonitorSettings();
        setMessage({
          type: "success",
          text: t("currentSectionResetSuccess", "Current section has been reset."),
        });
        setTimeout(() => setMessage({ type: "", text: "" }), 3000);
        return;
      }

      if (activeSubTab === "cloud-agent") {
        const confirmed = await confirmDialog(
          t(
            "confirmResetSettingsSection",
            "Discard unsaved changes in this settings section and reload the saved values?",
          ),
          {
            title: t("confirmResetSettingsSectionTitle", "Reset settings"),
            okLabel: t("reset", "Reset"),
            cancelLabel: t("cancel", "Cancel"),
            kind: "warning",
          },
        );
        if (!confirmed) return;
        await loadCloudAgentSettings?.();
        setMessage({
          type: "success",
          text: t("currentSectionResetSuccess", "Current section has been reset."),
        });
        setTimeout(() => setMessage({ type: "", text: "" }), 3000);
        return;
      }

      const otherTabsDirtyBeforeReset = ALL_LEGACY_SECTIONS.some(
        (tab) => tab !== activeSubTab && tabDirtyMap[tab],
      );
      const confirmed = await confirmDialog(
        t(
          "confirmResetSettingsSection",
          "Discard unsaved changes in this settings section and reload the saved values?",
        ),
        {
          title: t("confirmResetSettingsSectionTitle", "Reset settings"),
          okLabel: t("reset", "Reset"),
          cancelLabel: t("cancel", "Cancel"),
          kind: "warning",
        },
      );
      if (!confirmed) return;

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
        "currentSectionResetSuccess",
        "Current section has been reset.",
      );
      await notifyMessage.success({
        source: "settings",
        category: "reset",
        title: t("settingsSectionResetTitle", "Settings reset"),
        summary: baseText,
        dedupeKey: `settings:reset:${activeSubTab}`,
        target: { tab: "settings", section: activeSubTab },
      });
      pushToast({
        title: t("settingsSectionResetTitle", "Settings reset"),
        description: baseText,
        kind: "success",
      });
      setMessage({
        type: "success",
        text: otherTabsDirtyBeforeReset
          ? `${baseText} ${t("otherSectionsUnsaved", "Other sections still have unsaved changes.")}`
          : baseText,
      });
      setTimeout(() => setMessage({ type: "", text: "" }), 3000);
    } catch (e: any) {
      pushToast({
        title: t("settingsResetFailedTitle", "Failed to reset settings"),
        description: e.toString(),
        kind: "error",
      });
      setMessage({ type: "error", text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  return handleResetCurrentTab;
}
