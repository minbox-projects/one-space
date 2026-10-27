import { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import {
  ChevronLeft,
  Settings as SettingsIcon,
  CheckCircle2,
  AlertCircle,
  Undo2,
  Save,
} from "lucide-react";
import { useTheme } from "../ThemeProvider";
import { useToast } from "../ToastProvider";
import { useConfirmDialog } from "../ConfirmDialogProvider";
import { errorToMessage, notifyMessage, safeRecordMessage } from "@/lib/messages";
import { runUserAction } from "@/lib/userActions";
import {
  protocolRouterGetConfig,
  protocolRouterRotateToken,
  protocolRouterSaveConfig,
  protocolRouterStatus,
  type ProtocolRouterConfig,
  type ProtocolRouterStatus,
} from "@/lib/protocolRouter";
import {
  aiGatewayGetConfig,
  aiGatewaySaveSettings,
  aiGatewayTemplateAutoRefreshGet,
  aiGatewayTemplateAutoRefreshSave,
  aiGatewayUsageRetentionGet,
  aiGatewayUsageRetentionSave,
  notifyTemplateAutoRefreshIntervalChanged,
  normalizeGatewaySettings,
  DEFAULT_GATEWAY_SETTINGS,
  type GatewaySettings,
} from "@/lib/aiGateway";
import { sslGetConfig, sslSaveConfig, type SslConfig } from "@/lib/sslCertificate";
import {
  DEFAULT_SERVER_MONITOR_SETTINGS,
  normalizeServerMonitorSettings,
  serverMonitorGetSettings,
  serverMonitorSaveSettings,
  validateServerMonitorSettings,
  type ServerMonitorSettings,
} from "@/lib/serverMonitor";
import {
  type MainSettingsTab,
  type LegacySettingsSection,
  type StorageConfig,
  type ProxyConfig,
  type ProxyStatus,
  type SkillsSyncState,
  type SubagentSourceDiagnoseResult,
  resolveSettingsTab,
  DEFAULT_PROXY_CONFIG,
  DEFAULT_PROTOCOL_ROUTER_CONFIG,
  MAIN_SETTINGS_CATEGORIES,
} from "./types";
import {
  normalizeConfigForUi,
  normalizeProxyConfigForUi,
  normalizeProtocolRouterConfigForUi,
  normalizeSyncPolicyForUi,
  normalizeAiModelLaunchCommandsForUi,
  normalizeAiModelPermissionModesForUi,
  normalizeAiNewsRssSourcesForCompare,
  detectNewsRetentionPreset,
  formatAiNewsSyncIntervalInput,
  parseAiNewsSyncIntervalInput,
  parseUsageRetentionInput,
  parseTemplateAutoRefreshInput,
  generateRandomMd5String,
  getTabSnapshot,
  normalizeProtocolRouterForCompare,
  type NewsRetentionPreset,
} from "./helpers";
import { SettingsSidebar } from "./SettingsSidebar";
import { GeneralSection } from "./sections/GeneralSection";
import { AiEcoSection } from "./sections/AiEcoSection";
import { ServicesSection } from "./sections/ServicesSection";
import { DataSecuritySection } from "./sections/DataSecuritySection";
import { NewsSection } from "./sections/NewsSection";

const ALL_LEGACY_SECTIONS: LegacySettingsSection[] = [
  "storage",
  "news",
  "general",
  "updates",
  "skills",
  "subagents",
  "proxy",
  "protocol-router",
  "ai-gateway",
  "ssl-certificate",
  "server-monitor",
  "shortcuts",
  "ai",
  "appearance",
  "security",
];

function parseBoundedInteger(
  value: string,
  minimum: number,
  maximum: number,
): number | null {
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed < minimum || parsed > maximum) {
    return null;
  }
  return parsed;
}

export interface SettingsContainerProps {
  initialTab?: string;
  onBack?: () => void;
}

export function SettingsContainer({
  initialTab = "general",
  onBack,
}: SettingsContainerProps) {
  const { t } = useTranslation();
  const { theme, setTheme } = useTheme();
  const { pushToast } = useToast();
  const confirmDialog = useConfirmDialog();

  const initialRoute = useMemo(
    () => resolveSettingsTab(initialTab),
    [initialTab],
  );

  const [activeMainTab, setActiveMainTab] = useState<MainSettingsTab>(
    initialRoute.mainTab,
  );
  const [activeSubTab, setActiveSubTab] = useState<LegacySettingsSection>(
    initialRoute.subTab,
  );

  useEffect(() => {
    const route = resolveSettingsTab(initialTab);
    setActiveMainTab(route.mainTab);
    setActiveSubTab(route.subTab);
  }, [initialTab]);

  // Main configuration states
  const [config, setConfig] = useState<StorageConfig>({
    storage_type: "local",
  });
  const [savedConfig, setSavedConfig] = useState<StorageConfig>({
    storage_type: "local",
  });
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: "", text: "" });

  // News states
  const [newsRetentionPreset, setNewsRetentionPreset] =
    useState<NewsRetentionPreset>("90d_1000");
  const [aiNewsSyncIntervalInput, setAiNewsSyncIntervalInput] = useState("60");
  const [savedAiNewsSyncIntervalInput, setSavedAiNewsSyncIntervalInput] =
    useState("60");

  // Shortcut Recording States
  const [recordingField, setRecordingField] = useState<"main" | "quick" | null>(
    null,
  );

  // Security States
  const [masterPassword, setMasterPassword] = useState("");
  const [showPass, setShowPass] = useState(false);
  const [newPass, setNewPass] = useState("");
  const [confirmNewPass, setConfirmNewPass] = useState("");
  const [showNewPass, setShowNewPass] = useState(true);
  const [showConfirmNewPass, setShowConfirmNewPass] = useState(true);
  const [changingPass, setChangingPass] = useState(false);

  // Proxy States
  const [proxyConfig, setProxyConfig] = useState<ProxyConfig>(DEFAULT_PROXY_CONFIG);
  const [savedProxyConfig, setSavedProxyConfig] = useState<ProxyConfig>(DEFAULT_PROXY_CONFIG);
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [testingProxy, setTestingProxy] = useState(false);
  const [authEnabled, setAuthEnabled] = useState(false);

  // Protocol Router States
  const [protocolRouterConfig, setProtocolRouterConfig] =
    useState<ProtocolRouterConfig>(DEFAULT_PROTOCOL_ROUTER_CONFIG);
  const [savedProtocolRouterConfig, setSavedProtocolRouterConfig] =
    useState<ProtocolRouterConfig>(DEFAULT_PROTOCOL_ROUTER_CONFIG);
  const [protocolRouterStatusState, setProtocolRouterStatusState] =
    useState<ProtocolRouterStatus | null>(null);
  const [protocolRouterBusy, setProtocolRouterBusy] = useState(false);

  // AI Gateway States
  const [gatewaySettings, setGatewaySettings] = useState<GatewaySettings>(
    DEFAULT_GATEWAY_SETTINGS,
  );
  const [savedGatewaySettings, setSavedGatewaySettings] =
    useState<GatewaySettings>(DEFAULT_GATEWAY_SETTINGS);
  const [gatewayPortInput, setGatewayPortInput] = useState("17688");
  const [savedGatewayPort, setSavedGatewayPort] = useState(17688);
  const [usageRetentionInput, setUsageRetentionInput] = useState("90");
  const [savedUsageRetentionDays, setSavedUsageRetentionDays] = useState(90);
  const [templateAutoRefreshInput, setTemplateAutoRefreshInput] = useState("60");
  const [savedTemplateAutoRefreshMinutes, setSavedTemplateAutoRefreshMinutes] = useState(60);

  // SSL Certificate States
  const [sslConfig, setSslConfig] = useState<SslConfig>({
    autoCheckEnabled: true,
    checkTimes: ["09:00"],
    alertDaysThreshold: 30,
  });
  const [savedSslConfig, setSavedSslConfig] = useState<SslConfig>({
    autoCheckEnabled: true,
    checkTimes: ["09:00"],
    alertDaysThreshold: 30,
  });
  const [newCheckTimeInput, setNewCheckTimeInput] = useState("09:00");
  const [sslAlertDaysInput, setSslAlertDaysInput] = useState("30");

  // Server Monitor States
  const [serverMonitorSettings, setServerMonitorSettings] =
    useState<ServerMonitorSettings>(DEFAULT_SERVER_MONITOR_SETTINGS);
  const [savedServerMonitorSettings, setSavedServerMonitorSettings] =
    useState<ServerMonitorSettings>(DEFAULT_SERVER_MONITOR_SETTINGS);
  const [serverMonitorIntervalInput, setServerMonitorIntervalInput] = useState("5");
  const [serverMonitorRetentionInput, setServerMonitorRetentionInput] = useState("7");
  const [serverMonitorNewTimeInput, setServerMonitorNewTimeInput] = useState("");

  // Skills & Subagents States
  const [skillsSyncState, setSkillsSyncState] = useState<SkillsSyncState | null>(null);
  const [skillsSyncNowLoading, setSkillsSyncNowLoading] = useState(false);
  const [subagentsSyncState, setSubagentsSyncState] = useState<SkillsSyncState | null>(null);
  const [subagentsSyncNowLoading, setSubagentsSyncNowLoading] = useState(false);
  const [subagentSourceDiagnosing, setSubagentSourceDiagnosing] = useState<Record<string, boolean>>({});
  const [subagentSourceDiagnostics, setSubagentSourceDiagnostics] = useState<Record<string, SubagentSourceDiagnoseResult>>({});

  const actionContext = useMemo(
    () => ({
      t,
      confirm: confirmDialog,
      pushToast,
      recordMessage: safeRecordMessage,
    }),
    [confirmDialog, pushToast, t],
  );

  // Autostart helper
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

  // Keyboard shortcut listener
  useEffect(() => {
    if (!recordingField) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const modifiers = [];
      if (e.ctrlKey) modifiers.push("Control");
      if (e.altKey) modifiers.push("Alt");
      if (e.shiftKey) modifiers.push("Shift");
      if (e.metaKey) modifiers.push("Command");

      const key = e.key === " " ? "Space" : e.key;
      const isModifierOnly = ["Control", "Alt", "Shift", "Meta"].includes(e.key);

      if (!isModifierOnly) {
        let finalShortcut = "";
        if (modifiers.length > 0) {
          finalShortcut =
            modifiers.join("+") +
            "+" +
            key.charAt(0).toUpperCase() +
            key.slice(1);
        } else {
          finalShortcut = key.charAt(0).toUpperCase() + key.slice(1);
        }

        if (recordingField === "main") {
          setConfig((prev) => ({ ...prev, main_shortcut: finalShortcut }));
        } else {
          setConfig((prev) => ({ ...prev, quick_ai_shortcut: finalShortcut }));
        }
        setRecordingField(null);
      }
    };

    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [recordingField]);

  // Initial loading
  useEffect(() => {
    void loadConfig();
    void loadProtocolRouter();
    void loadAiGateway();
    void loadSslConfig();
    void loadServerMonitorSettings();
  }, []);

  useEffect(() => {
    if (activeSubTab === "security") {
      void loadMasterPassword();
    }
  }, [activeSubTab]);

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

  const handleTestProxy = async () => {
    setTestingProxy(true);
    setProxyStatus(null);
    try {
      const status = await invoke<ProxyStatus>("test_proxy_connection", {
        config: proxyConfig,
      });
      setProxyStatus(status);
    } catch (err: any) {
      setProxyStatus({
        is_available: false,
        latency_ms: 0,
        message: err.toString(),
        proxy_type: proxyConfig.proxy_type,
        proxy_host: proxyConfig.proxy_host,
      });
    } finally {
      setTestingProxy(false);
    }
  };

  const rotateProtocolToken = async () => {
    setProtocolRouterBusy(true);
    try {
      const nextConfig = await protocolRouterRotateToken();
      setProtocolRouterConfig((prev) => ({ ...prev, token: nextConfig.token }));
      pushToast({
        title: t("tokenRotatedTitle", "Token updated"),
        description: t(
          "tokenRotatedDesc",
          "Generated new router token. Save to apply.",
        ),
        kind: "success",
      });
    } catch (e: any) {
      pushToast({
        title: t("tokenRotateFailedTitle", "Token rotate failed"),
        description: errorToMessage(e),
        kind: "error",
      });
    } finally {
      setProtocolRouterBusy(false);
    }
  };

  const handleGenerateMd5Password = () => {
    const generated = generateRandomMd5String();
    setNewPass(generated);
    setConfirmNewPass(generated);
    setShowNewPass(true);
    setShowConfirmNewPass(true);
    setMessage({
      type: "success",
      text: t(
        "md5PasswordGenerated",
        "Generated and filled into both password fields.",
      ),
    });
    setTimeout(() => setMessage({ type: "", text: "" }), 2000);
  };

  const handleChangeMasterPassword = async () => {
    if (!newPass || newPass !== confirmNewPass) return;
    setLoading(true);
    try {
      await invoke("change_master_password", {
        oldPassword: masterPassword,
        newPassword: newPass,
      });
      setMasterPassword(newPass);
      setChangingPass(false);
      setNewPass("");
      setConfirmNewPass("");
      setMessage({
        type: "success",
        text: t("masterPasswordUpdated", "Master password updated successfully."),
      });
      setTimeout(() => setMessage({ type: "", text: "" }), 3000);
    } catch (e: any) {
      setMessage({ type: "error", text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  const handleSkillsSyncNow = async () => {
    setSkillsSyncNowLoading(true);
    try {
      await invoke("skills_sync_now");
      const resp = await invoke<{ ok: boolean; data: SkillsSyncState }>(
        "skills_sync_status_get",
      );
      setSkillsSyncState(resp?.data || null);
      setMessage({ type: "success", text: t("syncSuccess", "Sync successful") });
      setTimeout(() => setMessage({ type: "", text: "" }), 3000);
    } catch (e: any) {
      setMessage({ type: "error", text: e.toString() });
    } finally {
      setSkillsSyncNowLoading(false);
    }
  };

  const handleSubagentsSyncNow = async () => {
    setSubagentsSyncNowLoading(true);
    try {
      await invoke("subagents_sync_now");
      const resp = await invoke<{ ok: boolean; data: SkillsSyncState }>(
        "subagents_sync_status_get",
      );
      setSubagentsSyncState(resp?.data || null);
      setMessage({ type: "success", text: t("syncSuccess", "Sync successful") });
      setTimeout(() => setMessage({ type: "", text: "" }), 3000);
    } catch (e: any) {
      setMessage({ type: "error", text: e.toString() });
    } finally {
      setSubagentsSyncNowLoading(false);
    }
  };

  const handleDiagnoseSubagentSource = async (sourceId: string) => {
    if (!sourceId) return;
    setSubagentSourceDiagnosing((prev) => ({ ...prev, [sourceId]: true }));
    try {
      const resp = await invoke<{ ok: boolean; data: SubagentSourceDiagnoseResult }>(
        "subagents_source_diagnose",
        { sourceId },
      );
      if (resp?.data) {
        setSubagentSourceDiagnostics((prev) => ({
          ...prev,
          [sourceId]: resp.data,
        }));
      }
    } catch (e: any) {
      pushToast({
        title: "Diagnose failed",
        description: e.toString(),
        kind: "error",
      });
    } finally {
      setSubagentSourceDiagnosing((prev) => ({ ...prev, [sourceId]: false }));
    }
  };

  // Tab dirty calculation
  const tabDirtyMap = useMemo<Record<string, boolean>>(() => {
    const next: Record<string, boolean> = {};

    ALL_LEGACY_SECTIONS.forEach((tab) => {
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
        const alertDirty =
          serverMonitorSettings.alertEnabled !==
            savedServerMonitorSettings.alertEnabled ||
          serverMonitorSettings.cpuThresholdPercent !==
            savedServerMonitorSettings.cpuThresholdPercent ||
          serverMonitorSettings.memoryThresholdPercent !==
            savedServerMonitorSettings.memoryThresholdPercent ||
          serverMonitorSettings.diskThresholdPercent !==
            savedServerMonitorSettings.diskThresholdPercent ||
          serverMonitorSettings.alertCooldownMinutes !==
            savedServerMonitorSettings.alertCooldownMinutes;
        next[tab] =
          scheduleDirty ||
          alertDirty ||
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
  }, [
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
  ]);

  const currentTabDirty = tabDirtyMap[activeSubTab];

  const buildPayloadForTab = (
    tab: LegacySettingsSection,
    draftCfg: StorageConfig,
    draftProxy: ProxyConfig,
    baseCfg: StorageConfig,
  ): StorageConfig => {
    const next: StorageConfig = {
      ...baseCfg,
      skills_sources: [...(baseCfg.skills_sources || [])],
      subagents_sources: [...(baseCfg.subagents_sources || [])],
    };

    switch (tab) {
      case "storage":
        next.storage_type = draftCfg.storage_type;
        next.git_url = draftCfg.git_url;
        next.auth_method = draftCfg.auth_method;
        next.http_username = draftCfg.http_username;
        next.http_token = draftCfg.http_token;
        next.ssh_key_path = draftCfg.ssh_key_path;
        next.local_storage_path = draftCfg.local_storage_path;
        next.icloud_storage_path = draftCfg.icloud_storage_path;
        next.sync_policy = normalizeSyncPolicyForUi(draftCfg.sync_policy);
        break;
      case "news":
        next.ai_news_enabled = draftCfg.ai_news_enabled;
        next.ai_news_sync_interval_minutes =
          draftCfg.ai_news_sync_interval_minutes;
        next.ai_news_retention_days = draftCfg.ai_news_retention_days;
        next.ai_news_retention_max_items = draftCfg.ai_news_retention_max_items;
        next.ai_news_keywords = draftCfg.ai_news_keywords;
        next.ai_news_rss_sources = normalizeAiNewsRssSourcesForCompare(
          draftCfg.ai_news_rss_sources || [],
        );
        break;
      case "updates":
        next.auto_update_enabled = draftCfg.auto_update_enabled;
        next.update_check_interval_minutes =
          draftCfg.update_check_interval_minutes;
        break;
      case "general":
        next.launch_at_login = draftCfg.launch_at_login;
        next.message_retention_days = draftCfg.message_retention_days;
        next.default_sftp_download_dir = draftCfg.default_sftp_download_dir;
        break;
      case "skills":
        next.skills_sync_enabled = draftCfg.skills_sync_enabled;
        next.skills_auto_update_enabled = draftCfg.skills_auto_update_enabled;
        next.skills_sync_interval_minutes =
          draftCfg.skills_sync_interval_minutes;
        next.skills_new_badge_hours = draftCfg.skills_new_badge_hours;
        next.skills_sources = [...(draftCfg.skills_sources || [])];
        break;
      case "subagents":
        next.subagents_sync_enabled = draftCfg.subagents_sync_enabled;
        next.subagents_sync_interval_minutes =
          draftCfg.subagents_sync_interval_minutes;
        next.subagents_new_badge_hours = draftCfg.subagents_new_badge_hours;
        next.subagents_sources = [...(draftCfg.subagents_sources || [])];
        break;
      case "proxy":
        next.proxy = { ...draftProxy };
        break;
      case "shortcuts":
        next.main_shortcut = draftCfg.main_shortcut;
        next.quick_ai_shortcut = draftCfg.quick_ai_shortcut;
        break;
      case "ai":
        next.default_ai_model = draftCfg.default_ai_model;
        next.ai_terminal_app = draftCfg.ai_terminal_app;
        next.default_ai_dir = draftCfg.default_ai_dir;
        next.claude_provider_launch_dir = draftCfg.claude_provider_launch_dir;
        next.ai_model_launch_commands = normalizeAiModelLaunchCommandsForUi(
          draftCfg.ai_model_launch_commands,
        );
        next.ai_model_permission_modes = normalizeAiModelPermissionModesForUi(
          draftCfg.ai_model_permission_modes,
        );
        next.ai_sessions_history_days = draftCfg.ai_sessions_history_days;
        break;
      case "appearance":
        next.language = draftCfg.language;
        break;
      default:
        break;
    }

    return next;
  };

  const syncDraftWithLatestForTab = (
    tab: LegacySettingsSection,
    latestCfg: StorageConfig,
    latestProxy: ProxyConfig,
  ) => {
    if (tab === "proxy") {
      setProxyConfig(latestProxy);
      setAuthEnabled(
        !!(latestProxy.proxy_username || latestProxy.proxy_password),
      );
      return;
    }

    if (tab === "news") {
      setNewsRetentionPreset(
        detectNewsRetentionPreset(
          latestCfg.ai_news_retention_days,
          latestCfg.ai_news_retention_max_items,
        ),
      );
      const nextIntervalInput = formatAiNewsSyncIntervalInput(
        latestCfg.ai_news_sync_interval_minutes,
      );
      setAiNewsSyncIntervalInput(nextIntervalInput);
      setSavedAiNewsSyncIntervalInput(nextIntervalInput);
    }

    setConfig((prev) => {
      const next = { ...prev };
      switch (tab) {
        case "storage":
          next.storage_type = latestCfg.storage_type;
          next.git_url = latestCfg.git_url;
          next.auth_method = latestCfg.auth_method;
          next.http_username = latestCfg.http_username;
          next.http_token = latestCfg.http_token;
          next.ssh_key_path = latestCfg.ssh_key_path;
          next.local_storage_path = latestCfg.local_storage_path;
          next.icloud_storage_path = latestCfg.icloud_storage_path;
          next.sync_policy = normalizeSyncPolicyForUi(latestCfg.sync_policy);
          break;
        case "news":
          next.ai_news_enabled = latestCfg.ai_news_enabled;
          next.ai_news_sync_interval_minutes =
            latestCfg.ai_news_sync_interval_minutes;
          next.ai_news_retention_days = latestCfg.ai_news_retention_days;
          next.ai_news_retention_max_items =
            latestCfg.ai_news_retention_max_items;
          next.ai_news_keywords = latestCfg.ai_news_keywords;
          next.ai_news_rss_sources = [
            ...(latestCfg.ai_news_rss_sources || []),
          ];
          break;
        case "updates":
          next.auto_update_enabled = latestCfg.auto_update_enabled;
          next.update_check_interval_minutes =
            latestCfg.update_check_interval_minutes;
          break;
        case "general":
          next.launch_at_login = latestCfg.launch_at_login;
          next.message_retention_days = latestCfg.message_retention_days;
          next.default_sftp_download_dir = latestCfg.default_sftp_download_dir;
          break;
        case "skills":
          next.skills_sync_enabled = latestCfg.skills_sync_enabled;
          next.skills_auto_update_enabled = latestCfg.skills_auto_update_enabled;
          next.skills_sync_interval_minutes =
            latestCfg.skills_sync_interval_minutes;
          next.skills_new_badge_hours = latestCfg.skills_new_badge_hours;
          next.skills_sources = [...(latestCfg.skills_sources || [])];
          break;
        case "subagents":
          next.subagents_sync_enabled = latestCfg.subagents_sync_enabled;
          next.subagents_sync_interval_minutes =
            latestCfg.subagents_sync_interval_minutes;
          next.subagents_new_badge_hours = latestCfg.subagents_new_badge_hours;
          next.subagents_sources = [...(latestCfg.subagents_sources || [])];
          break;
        case "shortcuts":
          next.main_shortcut = latestCfg.main_shortcut;
          next.quick_ai_shortcut = latestCfg.quick_ai_shortcut;
          break;
        case "ai":
          next.default_ai_model = latestCfg.default_ai_model;
          next.ai_terminal_app = latestCfg.ai_terminal_app;
          next.default_ai_dir = latestCfg.default_ai_dir;
          next.claude_provider_launch_dir = latestCfg.claude_provider_launch_dir;
          next.ai_model_launch_commands = normalizeAiModelLaunchCommandsForUi(
            latestCfg.ai_model_launch_commands,
          );
          next.ai_model_permission_modes = normalizeAiModelPermissionModesForUi(
            latestCfg.ai_model_permission_modes,
          );
          next.ai_sessions_history_days = latestCfg.ai_sessions_history_days;
          break;
        case "appearance":
          next.language = latestCfg.language;
          break;
        default:
          break;
      }
      return next;
    });
  };

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
      syncDraftWithLatestForTab(activeSubTab, latestConfig, latestProxy);

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
      syncDraftWithLatestForTab(activeSubTab, latestConfig, latestProxy);

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

  const handleSelectMainTab = (mainTab: MainSettingsTab) => {
    setActiveMainTab(mainTab);
    const cat = MAIN_SETTINGS_CATEGORIES.find((c) => c.id === mainTab);
    if (cat && cat.subTabs.length > 0) {
      const currentSubTabValid = cat.subTabs.some((st) => st.id === activeSubTab);
      if (!currentSubTabValid) {
        setActiveSubTab(cat.subTabs[0].id);
      }
    }
  };

  return (
    <div className="flex h-full flex-col bg-background animate-in fade-in duration-200">
      {/* Top Header bar with Back button, Title, status feedback */}
      <div className="flex items-center justify-between px-6 py-3.5 border-b shrink-0 bg-card/40 backdrop-blur-sm sticky top-0 z-10">
        <div className="flex items-center gap-3">
          {onBack && (
            <button
              type="button"
              onClick={onBack}
              className="p-2 rounded-full hover:bg-muted text-muted-foreground transition-all active:scale-95"
              title={t("back", "Back")}
            >
              <ChevronLeft className="w-5 h-5" />
            </button>
          )}
          <div className="flex items-center gap-2">
            <SettingsIcon className="w-5 h-5 text-primary" />
            <h1 className="text-lg font-bold tracking-tight">
              {t("settings", "Settings")}
            </h1>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {message.text && (
            <div
              className={`flex items-center gap-2 px-3 py-1.5 rounded-full text-xs font-medium animate-in zoom-in-95 ${
                message.type === "error"
                  ? "bg-destructive/10 text-destructive border border-destructive/20"
                  : "bg-green-500/10 text-green-600 border border-green-500/20"
              }`}
            >
              {message.type === "error" ? (
                <AlertCircle className="w-3.5 h-3.5" />
              ) : (
                <CheckCircle2 className="w-3.5 h-3.5" />
              )}
              {message.text}
            </div>
          )}
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* Left 5-category Sidebar */}
        <SettingsSidebar
          activeMainTab={activeMainTab}
          onSelectMainTab={handleSelectMainTab}
          subTabDirtyMap={tabDirtyMap}
        />

        {/* Right Content Area */}
        <div className="flex-1 bg-background/50 flex min-w-0 flex-col">
          {/* Section Toolbar with dirty indicator, reset & save buttons */}
          <div className="shrink-0 border-b bg-card/70 backdrop-blur-sm px-8 py-3">
            <div className="max-w-4xl mx-auto flex items-center justify-between gap-3">
              <div
                className={`text-xs font-medium inline-flex items-center gap-2 ${
                  currentTabDirty ? "text-amber-700 dark:text-amber-400" : "text-muted-foreground"
                }`}
              >
                {currentTabDirty ? (
                  <AlertCircle className="w-3.5 h-3.5 text-amber-500" />
                ) : (
                  <CheckCircle2 className="w-3.5 h-3.5 text-emerald-500" />
                )}
                {currentTabDirty
                  ? t("currentSectionUnsaved", "Unsaved changes in this section")
                  : t("currentSectionSaved", "No unsaved changes in this section")}
              </div>

              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={handleResetCurrentTab}
                  disabled={loading || !currentTabDirty || activeSubTab === "security"}
                  className="px-3 py-1.5 rounded-lg border bg-background hover:bg-muted text-xs font-medium text-muted-foreground hover:text-foreground disabled:opacity-40 transition-all inline-flex items-center gap-1.5"
                >
                  <Undo2 className="w-3.5 h-3.5" />
                  <span>{t("resetCurrentTab", "Reset")}</span>
                </button>
                <button
                  type="button"
                  onClick={handleSaveCurrentTab}
                  disabled={loading || !currentTabDirty || activeSubTab === "security"}
                  className="px-3.5 py-1.5 rounded-lg bg-primary hover:bg-primary/90 text-primary-foreground text-xs font-medium disabled:opacity-40 shadow-sm transition-all inline-flex items-center gap-1.5"
                >
                  <Save className="w-3.5 h-3.5" />
                  <span>{t("saveCurrentTab", "Save Settings")}</span>
                </button>
              </div>
            </div>
          </div>

          {/* Section Body */}
          <div className="flex-1 overflow-y-auto px-8 py-6">
            <div className="max-w-4xl mx-auto">
              {activeMainTab === "general" && (
                <GeneralSection
                  subTab={activeSubTab}
                  onSubTabChange={setActiveSubTab}
                  config={config}
                  setConfig={setConfig}
                  theme={theme}
                  setTheme={setTheme}
                  recordingField={recordingField}
                  setRecordingField={setRecordingField}
                  subTabDirtyMap={tabDirtyMap}
                />
              )}

              {activeMainTab === "ai" && (
                <AiEcoSection
                  subTab={activeSubTab}
                  onSubTabChange={setActiveSubTab}
                  config={config}
                  setConfig={setConfig}
                  loading={loading}
                  skillsSyncNowLoading={skillsSyncNowLoading}
                  onSkillsSyncNow={handleSkillsSyncNow}
                  skillsSyncState={skillsSyncState}
                  subagentsSyncNowLoading={subagentsSyncNowLoading}
                  onSubagentsSyncNow={handleSubagentsSyncNow}
                  subagentsSyncState={subagentsSyncState}
                  subagentSourceDiagnosing={subagentSourceDiagnosing}
                  subagentSourceDiagnostics={subagentSourceDiagnostics}
                  onDiagnoseSubagentSource={handleDiagnoseSubagentSource}
                  onShowToast={(type, text) => {
                    setMessage({ type, text });
                    setTimeout(() => setMessage({ type: "", text: "" }), 3000);
                  }}
                  subTabDirtyMap={tabDirtyMap}
                />
              )}

              {activeMainTab === "services" && (
                <ServicesSection
                  subTab={activeSubTab}
                  onSubTabChange={setActiveSubTab}
                  proxyConfig={proxyConfig}
                  setProxyConfig={setProxyConfig}
                  proxyStatus={proxyStatus}
                  testingProxy={testingProxy}
                  onTestProxy={handleTestProxy}
                  authEnabled={authEnabled}
                  setAuthEnabled={setAuthEnabled}
                  protocolRouterConfig={protocolRouterConfig}
                  setProtocolRouterConfig={setProtocolRouterConfig}
                  protocolRouterStatusState={protocolRouterStatusState}
                  protocolRouterBusy={protocolRouterBusy}
                  onRotateProtocolToken={rotateProtocolToken}
                  usageRetentionInput={usageRetentionInput}
                  setUsageRetentionInput={setUsageRetentionInput}
                  templateAutoRefreshInput={templateAutoRefreshInput}
                  setTemplateAutoRefreshInput={setTemplateAutoRefreshInput}
                  gatewaySettings={gatewaySettings}
                  setGatewaySettings={setGatewaySettings}
                  gatewayPortInput={gatewayPortInput}
                  setGatewayPortInput={setGatewayPortInput}
                  sslConfig={sslConfig}
                  setSslConfig={setSslConfig}
                  newCheckTimeInput={newCheckTimeInput}
                  setNewCheckTimeInput={setNewCheckTimeInput}
                  sslAlertDaysInput={sslAlertDaysInput}
                  setSslAlertDaysInput={setSslAlertDaysInput}
                  serverMonitorSettings={serverMonitorSettings}
                  setServerMonitorSettings={setServerMonitorSettings}
                  serverMonitorIntervalInput={serverMonitorIntervalInput}
                  setServerMonitorIntervalInput={setServerMonitorIntervalInput}
                  serverMonitorRetentionInput={serverMonitorRetentionInput}
                  setServerMonitorRetentionInput={setServerMonitorRetentionInput}
                  serverMonitorNewTimeInput={serverMonitorNewTimeInput}
                  setServerMonitorNewTimeInput={setServerMonitorNewTimeInput}
                  onShowToast={(type, text) => {
                    setMessage({ type, text });
                    setTimeout(() => setMessage({ type: "", text: "" }), 3000);
                  }}
                  subTabDirtyMap={tabDirtyMap}
                />
              )}

              {activeMainTab === "data-security" && (
                <DataSecuritySection
                  subTab={activeSubTab}
                  onSubTabChange={setActiveSubTab}
                  config={config}
                  setConfig={setConfig}
                  masterPassword={masterPassword}
                  showPass={showPass}
                  setShowPass={setShowPass}
                  newPass={newPass}
                  setNewPass={setNewPass}
                  confirmNewPass={confirmNewPass}
                  setConfirmNewPass={setConfirmNewPass}
                  showNewPass={showNewPass}
                  setShowNewPass={setShowNewPass}
                  showConfirmNewPass={showConfirmNewPass}
                  setShowConfirmNewPass={setShowConfirmNewPass}
                  changingPass={changingPass}
                  setChangingPass={setChangingPass}
                  onGenerateMd5Password={handleGenerateMd5Password}
                  onChangeMasterPassword={handleChangeMasterPassword}
                  loading={loading}
                  onShowToast={(type, text) => {
                    setMessage({ type, text });
                    setTimeout(() => setMessage({ type: "", text: "" }), 3000);
                  }}
                  subTabDirtyMap={tabDirtyMap}
                />
              )}

              {activeMainTab === "news" && (
                <NewsSection
                  config={config}
                  setConfig={setConfig}
                  aiNewsSyncIntervalInput={aiNewsSyncIntervalInput}
                  setAiNewsSyncIntervalInput={setAiNewsSyncIntervalInput}
                  newsRetentionPreset={newsRetentionPreset}
                  setNewsRetentionPreset={setNewsRetentionPreset}
                  lastAiNewsSyncText={
                    config.ai_news_last_synced_at
                      ? new Date(config.ai_news_last_synced_at).toLocaleString()
                      : undefined
                  }
                  onShowToast={(type, text) => {
                    setMessage({ type, text });
                    setTimeout(() => setMessage({ type: "", text: "" }), 3000);
                  }}
                />
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
