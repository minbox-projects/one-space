import { useState, useEffect, useMemo } from "react";
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
import { safeRecordMessage } from "@/lib/messages";
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
import type { NewsRetentionPreset } from "./helpers";
import { SettingsSidebar } from "./SettingsSidebar";
import { GeneralSection } from "./sections/GeneralSection";
import { AiEcoSection } from "./sections/AiEcoSection";
import { ServicesSection } from "./sections/ServicesSection";
import { DataSecuritySection } from "./sections/DataSecuritySection";
import { NewsSection } from "./sections/NewsSection";
import { createSettingsLoaders } from "./settingsContainer/loaders";
import { createSettingsActions } from "./settingsContainer/actions";
import { createSaveCurrentTabHandler } from "./settingsContainer/save";
import { createResetCurrentTabHandler } from "./settingsContainer/reset";
import { computeTabDirtyMap } from "./settingsContainer/tabDirty";
import type { DraftSyncSetters } from "./settingsContainer/draftSync";
import {
  DEFAULT_GATEWAY_SETTINGS,
  type GatewaySettings,
} from "@/lib/aiGateway";
import type {
  ProtocolRouterConfig,
  ProtocolRouterStatus,
} from "@/lib/protocolRouter";
import type { SslConfig } from "@/lib/sslCertificate";
import {
  DEFAULT_SERVER_MONITOR_SETTINGS,
  type ServerMonitorSettings,
} from "@/lib/serverMonitor";
import {
  DEFAULT_CLOUD_AGENT_SETTINGS,
  type CloudAgentSettings,
} from "@/lib/cloudAgent";

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

  // Cloud Agent States
  const [cloudAgentSettings, setCloudAgentSettings] =
    useState<CloudAgentSettings>(DEFAULT_CLOUD_AGENT_SETTINGS);
  const [savedCloudAgentSettings, setSavedCloudAgentSettings] =
    useState<CloudAgentSettings>(DEFAULT_CLOUD_AGENT_SETTINGS);

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

  const {
    getAutostartEnabled,
    setAutostartEnabled,
    loadConfig,
    loadMasterPassword,
    loadProtocolRouter,
    loadAiGateway,
    loadSslConfig,
    loadServerMonitorSettings,
    loadCloudAgentSettings,
  } = createSettingsLoaders({
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
  });

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
    void loadCloudAgentSettings();
  }, []);

  useEffect(() => {
    if (activeSubTab === "security") {
      void loadMasterPassword();
    }
  }, [activeSubTab]);

  // Tab dirty calculation
  const tabDirtyMap = useMemo<Record<string, boolean>>(
    () =>
      computeTabDirtyMap({
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
        cloudAgentSettings,
        savedCloudAgentSettings,
      }),
    [
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
      cloudAgentSettings,
      savedCloudAgentSettings,
    ],
  );

  const currentTabDirty = tabDirtyMap[activeSubTab];

  const draftSync: DraftSyncSetters = {
    setProxyConfig,
    setAuthEnabled,
    setNewsRetentionPreset,
    setAiNewsSyncIntervalInput,
    setSavedAiNewsSyncIntervalInput,
    setConfig,
  };

  const {
    handleTestProxy,
    rotateProtocolToken,
    handleGenerateMd5Password,
    handleChangeMasterPassword,
    handleSkillsSyncNow,
    handleSubagentsSyncNow,
    handleDiagnoseSubagentSource,
  } = createSettingsActions({
    t,
    pushToast,
    proxyConfig,
    setProxyStatus,
    setTestingProxy,
    setProtocolRouterBusy,
    setProtocolRouterConfig,
    setNewPass,
    setConfirmNewPass,
    setShowNewPass,
    setShowConfirmNewPass,
    setMessage,
    masterPassword,
    newPass,
    confirmNewPass,
    setLoading,
    setChangingPass,
    setMasterPassword,
    setSkillsSyncNowLoading,
    setSkillsSyncState,
    setSubagentsSyncNowLoading,
    setSubagentsSyncState,
    setSubagentSourceDiagnosing,
    setSubagentSourceDiagnostics,
  });

  const handleSaveCurrentTab = createSaveCurrentTabHandler({
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
    cloudAgentSettings,
    setSavedCloudAgentSettings,
  });

  const handleResetCurrentTab = createResetCurrentTabHandler({
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
  });

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
                  cloudAgentSettings={cloudAgentSettings}
                  setCloudAgentSettings={setCloudAgentSettings}
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
