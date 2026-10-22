import { useTranslation } from "react-i18next";
import {
  Globe,
  Route,
  Network,
  ShieldCheck,
  CheckCircle2,
  AlertCircle,
  Copy,
  Key,
  Clock,
  Plus,
  X,
  RefreshCw,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { SettingCard } from "../common/SettingCard";
import { SettingSelect } from "../common/SettingSelect";
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
import type { SslConfig } from "@/lib/sslCertificate";
import {
  type GatewaySettings,
  DEFAULT_GATEWAY_SETTINGS,
  normalizeGatewaySettings,
} from "@/lib/aiGateway";

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
  onShowToast,
  subTabDirtyMap = {},
}: ServicesSectionProps) {
  const { t } = useTranslation();
  const safeSettings = normalizeGatewaySettings(gatewaySettings);

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
        <SettingCard
          id="section-proxy"
          icon={Globe}
          title={t("proxySettings", "Network Proxy Settings")}
          description={t(
            "proxySettingsDesc",
            "Configure proxy for backend network requests",
          )}
        >
          {/* Proxy switch */}
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <h3 className="text-sm font-medium">
                {t("proxyEnabled", "Enable Proxy")}
              </h3>
              <p className="text-xs text-muted-foreground">
                {t(
                  "proxyEnabledDesc",
                  "All backend requests will use the proxy",
                )}
              </p>
            </div>
            <label className="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                className="sr-only peer"
                checked={proxyConfig.proxy_enabled}
                onChange={(e) =>
                  setProxyConfig((prev) => ({
                    ...prev,
                    proxy_enabled: e.target.checked,
                  }))
                }
              />
              <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/20 rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
            </label>
          </div>

          {proxyConfig.proxy_enabled && (
            <>
              <hr className="border-border/50 animate-in fade-in" />

              {/* Proxy Type */}
              <div className="space-y-2">
                <label className="text-sm font-medium">
                  {t("proxyType", "Proxy Type")}
                </label>
                <div className="grid grid-cols-3 gap-2 p-1 bg-muted rounded-xl border">
                  {(["http", "https", "socks5"] as const).map((type) => (
                    <button
                      key={type}
                      type="button"
                      onClick={() =>
                        setProxyConfig((prev) => ({
                          ...prev,
                          proxy_type: type,
                        }))
                      }
                      className={`py-2 px-3 rounded-lg text-xs sm:text-sm font-medium transition-all ${
                        proxyConfig.proxy_type === type
                          ? "bg-background shadow-sm text-foreground"
                          : "text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      {type.toUpperCase()}
                    </button>
                  ))}
                </div>
              </div>

              {/* Host and Port */}
              <div className="grid grid-cols-1 sm:grid-cols-[1fr_140px] gap-3">
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-muted-foreground">
                    {t("proxyHost", "Proxy Host")}
                  </label>
                  <input
                    type="text"
                    placeholder="127.0.0.1"
                    value={proxyConfig.proxy_host}
                    onChange={(e) =>
                      setProxyConfig((prev) => ({
                        ...prev,
                        proxy_host: e.target.value,
                      }))
                    }
                    className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-muted-foreground">
                    {t("proxyPort", "Port")}
                  </label>
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    placeholder="1080"
                    value={proxyConfig.proxy_port || ""}
                    onChange={(e) =>
                      setProxyConfig((prev) => ({
                        ...prev,
                        proxy_port: parseInt(e.target.value) || 0,
                      }))
                    }
                    className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                </div>
              </div>

              {/* Auth Credentials */}
              <div className="space-y-3 pt-1">
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium">
                    {t("proxyAuth", "Authentication (Optional)")}
                  </span>
                  <label className="relative inline-flex items-center cursor-pointer">
                    <input
                      type="checkbox"
                      className="sr-only peer"
                      checked={authEnabled}
                      onChange={(e) => {
                        setAuthEnabled(e.target.checked);
                        if (!e.target.checked) {
                          setProxyConfig((prev) => ({
                            ...prev,
                            proxy_username: "",
                            proxy_password: "",
                          }));
                        }
                      }}
                    />
                    <div className="w-9 h-5 bg-gray-200 peer-focus:outline-none rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
                  </label>
                </div>

                {authEnabled && (
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-1">
                    <div className="space-y-1">
                      <label className="text-xs text-muted-foreground">
                        {t("username", "Username")}
                      </label>
                      <input
                        type="text"
                        value={proxyConfig.proxy_username || ""}
                        onChange={(e) =>
                          setProxyConfig((prev) => ({
                            ...prev,
                            proxy_username: e.target.value,
                          }))
                        }
                        className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs text-muted-foreground">
                        {t("password", "Password")}
                      </label>
                      <input
                        type="password"
                        value={proxyConfig.proxy_password || ""}
                        onChange={(e) =>
                          setProxyConfig((prev) => ({
                            ...prev,
                            proxy_password: e.target.value,
                          }))
                        }
                        className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                      />
                    </div>
                  </div>
                )}
              </div>

              {/* Status & Test Button */}
              <div className="flex items-center justify-between border-t pt-4">
                <div className="text-xs">
                  {proxyStatus ? (
                    <span
                      className={`inline-flex items-center gap-1.5 font-medium ${
                        proxyStatus.is_available
                          ? "text-emerald-600 dark:text-emerald-400"
                          : "text-destructive"
                      }`}
                    >
                      {proxyStatus.is_available ? (
                        <CheckCircle2 className="w-3.5 h-3.5" />
                      ) : (
                        <AlertCircle className="w-3.5 h-3.5" />
                      )}
                      {proxyStatus.message ||
                        (proxyStatus.is_available
                          ? `${t("proxyAvailable", "Proxy connected")} (${proxyStatus.latency_ms}ms)`
                          : t("proxyFailed", "Connection failed"))}
                    </span>
                  ) : (
                    <span className="text-muted-foreground">
                      {t("proxyNotTested", "Proxy status not verified yet")}
                    </span>
                  )}
                </div>

                {onTestProxy && (
                  <button
                    type="button"
                    onClick={onTestProxy}
                    disabled={testingProxy || !proxyConfig.proxy_host}
                    className="px-3.5 py-1.5 rounded-xl border bg-background hover:bg-muted text-xs font-medium inline-flex items-center gap-1.5 transition-all disabled:opacity-50"
                  >
                    <RefreshCw
                      className={`w-3.5 h-3.5 ${testingProxy ? "animate-spin" : ""}`}
                    />
                    <span>{t("testProxy", "Test Proxy")}</span>
                  </button>
                )}
              </div>
            </>
          )}
        </SettingCard>
      )}

      {/* 2. Protocol Router */}
      {subTab === "protocol-router" && (
        <SettingCard
          id="section-protocol-router"
          icon={Route}
          title={t("protocolRouterSettings", "Protocol Router Settings")}
          description={t(
            "protocolRouterSettingsDesc",
            "Configure the local protocol router port, request retention, and router token. Route status, traffic details, and runtime control now live in the Protocol Router workspace.",
          )}
        >
          {/* Status grid */}
          <div className="grid gap-3 grid-cols-3">
            <div className="rounded-xl border bg-muted/20 p-3.5">
              <div className="text-xs uppercase tracking-wider text-muted-foreground font-semibold">
                {t("status", "Status")}
              </div>
              <div className="mt-1 text-base font-semibold">
                {protocolRouterStatusState?.running
                  ? t("running", "Running")
                  : t("stopped", "Stopped")}
              </div>
            </div>
            <div className="rounded-xl border bg-muted/20 p-3.5">
              <div className="text-xs uppercase tracking-wider text-muted-foreground font-semibold">
                {t("port", "Port")}
              </div>
              <div className="mt-1 text-base font-semibold">
                {protocolRouterConfig.port}
              </div>
            </div>
            <div className="rounded-xl border bg-muted/20 p-3.5">
              <div className="text-xs uppercase tracking-wider text-muted-foreground font-semibold">
                {t("routes", "Routes")}
              </div>
              <div className="mt-1 text-base font-semibold">
                {protocolRouterConfig.routes.length}
              </div>
            </div>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                {t("port", "Port")}
              </label>
              <input
                type="number"
                min={1}
                max={65535}
                value={protocolRouterConfig.port}
                onChange={(e) =>
                  setProtocolRouterConfig((prev) => ({
                    ...prev,
                    port: parseInt(e.target.value) || 17687,
                  }))
                }
                className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                {t("retentionDays", "Request History Retention Days")}
              </label>
              <input
                type="number"
                min={1}
                max={365}
                value={protocolRouterConfig.retention_days}
                onChange={(e) =>
                  setProtocolRouterConfig((prev) => ({
                    ...prev,
                    retention_days: Math.min(
                      365,
                      Math.max(1, parseInt(e.target.value) || 30),
                    ),
                  }))
                }
                className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
              />
            </div>
          </div>

          {/* Token Management */}
          <div className="space-y-2 rounded-xl border bg-muted/20 p-4">
            <div className="flex items-start justify-between gap-2">
              <div>
                <div className="text-xs font-semibold text-foreground">
                  {t("protocolRouterTokenLabel", "Router Token")}
                </div>
                <p className="text-xs text-muted-foreground mt-0.5">
                  {t(
                    "protocolRouterTokenDesc",
                    "Claude profiles use this token to access the local router.",
                  )}
                </p>
              </div>
              <div className="text-xs text-muted-foreground font-mono">
                {protocolRouterConfig.token
                  ? t("protocolRouterTokenReady", "Generated")
                  : t("protocolRouterTokenNotGenerated", "Not generated")}
              </div>
            </div>

            <div className="flex items-center gap-2 pt-1">
              <input
                readOnly
                value={protocolRouterConfig.token}
                placeholder={t("protocolRouterTokenNotGenerated", "Not generated")}
                className="flex-1 bg-background border rounded-xl px-3.5 py-2 font-mono text-xs cursor-default"
              />
              <button
                type="button"
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(
                      protocolRouterConfig.token,
                    );
                    onShowToast?.("success", t("copiedToClipboard", "Copied to clipboard"));
                  } catch (e: any) {
                    onShowToast?.("error", "Copy failed");
                  }
                }}
                disabled={!protocolRouterConfig.token}
                className="px-3 py-2 rounded-xl border bg-background hover:bg-muted text-xs font-medium inline-flex items-center gap-1.5 transition-all disabled:opacity-50"
              >
                <Copy className="h-3.5 w-3.5" />
                <span>{t("copy", "Copy")}</span>
              </button>
              {onRotateProtocolToken && (
                <button
                  type="button"
                  onClick={() => void onRotateProtocolToken()}
                  disabled={protocolRouterBusy}
                  className="px-3 py-2 rounded-xl border bg-background hover:bg-muted text-xs font-medium inline-flex items-center gap-1.5 transition-all disabled:opacity-50"
                >
                  <Key className="h-3.5 w-3.5" />
                  <span>{t("rotateToken", "Rotate Token")}</span>
                </button>
              )}
            </div>
          </div>

          <div className="rounded-xl border bg-muted/10 p-4 text-xs text-muted-foreground flex items-center justify-between gap-4">
            <div>
              <p className="font-semibold text-foreground">
                {t("protocolRouterOpenToolTitle", "Protocol Router workspace")}
              </p>
              <p className="mt-0.5">
                {t(
                  "protocolRouterOpenToolDesc",
                  "Open workspace to view runtime traffic, health, and request traces.",
                )}
              </p>
            </div>
            <button
              type="button"
              onClick={() => {
                const appWindow = window as unknown as {
                  setActiveTab?: (tab: string) => void;
                };
                appWindow.setActiveTab?.("protocol-router");
              }}
              className="px-3 py-1.5 rounded-lg border bg-background hover:bg-muted text-foreground text-xs font-medium inline-flex items-center gap-1.5 shrink-0"
            >
              <Route className="h-3.5 w-3.5" />
              <span>{t("openProtocolRouterTool", "Open Tool")}</span>
            </button>
          </div>
        </SettingCard>
      )}

      {/* 3. AI Gateway */}
      {subTab === "ai-gateway" && (
        <div className="space-y-6">
          {/* Group 1: Network & Access */}
          <SettingCard
            id="section-ai-gateway-network"
            icon={Network}
            title={t("aiGatewaySectionNetwork", "Network & Access Control")}
            description={t(
              "aiGatewaySectionNetworkDesc",
              "Configure listening port, network binding, LAN security, CORS, and upstream proxy.",
            )}
          >
            <div className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayPortLabel", "Listening Port")}
                  </span>
                  <input
                    type="number"
                    min={1024}
                    max={65535}
                    value={gatewayPortInput || "17688"}
                    onChange={(e) => setGatewayPortInput?.(e.target.value)}
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayPortDesc", "Port on which the local AI Gateway server listens (1024-65535).")}
                  </p>
                </label>

                <div className="space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayBindAddressLabel", "Bind Address")}
                  </span>
                  <SettingSelect
                    value={safeSettings.bind_address}
                    onChange={(val) =>
                      setGatewaySettings?.((prev) => ({
                        ...normalizeGatewaySettings(prev),
                        bind_address: val,
                      }))
                    }
                    options={[
                      {
                        value: "127.0.0.1",
                        label: t("aiGatewayBindLocal", "127.0.0.1 (Local Only)"),
                      },
                      {
                        value: "0.0.0.0",
                        label: t("aiGatewayBindAll", "0.0.0.0 (Allow LAN Access)"),
                      },
                    ]}
                    className="w-full"
                    ariaLabel={t("aiGatewayBindAddressLabel", "Bind Address")}
                    selectTestId="ai-gateway-bind-address-select"
                    triggerTestId="ai-gateway-bind-address-trigger"
                    optionTestIdPrefix="ai-gateway-bind-address-option"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayBindAddressDesc", "Local loopback restricts to this machine; 0.0.0.0 allows LAN devices to access.")}
                  </p>
                </div>
              </div>

              <hr className="border-border/50" />

              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <h4 className="text-sm font-medium text-foreground">
                    {t("aiGatewayRequireLanKeyLabel", "Require API Key for LAN Access")}
                  </h4>
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayRequireLanKeyDesc", "Enforce bearer token authentication when accessed across LAN.")}
                  </p>
                </div>
                <Switch
                  checked={safeSettings.require_api_key_for_lan}
                  onCheckedChange={(checked) =>
                    setGatewaySettings?.((prev) => ({
                      ...normalizeGatewaySettings(prev),
                      require_api_key_for_lan: checked,
                    }))
                  }
                />
              </div>

              <hr className="border-border/50" />

              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <h4 className="text-sm font-medium text-foreground">
                    {t("aiGatewayCorsLabel", "Enable CORS")}
                  </h4>
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayCorsDesc", "Allow browser-based frontends (Web UI / Web Chat) to connect directly with CORS headers.")}
                  </p>
                </div>
                <Switch
                  checked={safeSettings.enable_cors}
                  onCheckedChange={(checked) =>
                    setGatewaySettings?.((prev) => ({
                      ...normalizeGatewaySettings(prev),
                      enable_cors: checked,
                    }))
                  }
                />
              </div>

              <hr className="border-border/50" />

              <label className="block space-y-1.5">
                <span className="text-sm font-medium">
                  {t("aiGatewayProxyUrlLabel", "Upstream Proxy")}
                </span>
                <input
                  type="text"
                  placeholder="socks5://127.0.0.1:7890 或 http://127.0.0.1:7890"
                  value={safeSettings.proxy_url || ""}
                  onChange={(e) => {
                    const val = e.target.value.trim();
                    setGatewaySettings?.((prev) => ({
                      ...normalizeGatewaySettings(prev),
                      proxy_url: val ? val : null,
                    }));
                  }}
                  className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                <p className="text-xs text-muted-foreground">
                  {t("aiGatewayProxyUrlDesc", "HTTP, HTTPS or SOCKS5 proxy URL for forward requests. Leave blank to disable.")}
                </p>
              </label>
            </div>
          </SettingCard>

          {/* Group 2: Timeouts & Retries */}
          <SettingCard
            id="section-ai-gateway-timeouts"
            icon={Clock}
            title={t("aiGatewaySectionTimeouts", "Timeouts & Retry Policy")}
            description={t(
              "aiGatewaySectionTimeoutsDesc",
              "Manage request timeouts, TTFT buffer, retry backoff, and overall request budget.",
            )}
          >
            <div className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayConnectTimeoutLabel", "Connect Timeout (s)")}
                  </span>
                  <input
                    type="number"
                    min={1}
                    max={60}
                    value={safeSettings.connect_timeout_secs}
                    onChange={(e) =>
                      setGatewaySettings?.((prev) => ({
                        ...normalizeGatewaySettings(prev),
                        connect_timeout_secs: parseInt(e.target.value, 10) || 1,
                      }))
                    }
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayConnectTimeoutDesc", "TCP/TLS handshake timeout (1-60s).")}
                  </p>
                </label>

                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayReadTimeoutLabel", "Read / TTFT Timeout (s)")}
                  </span>
                  <input
                    type="number"
                    min={5}
                    max={3600}
                    value={safeSettings.read_timeout_secs}
                    onChange={(e) =>
                      setGatewaySettings?.((prev) => ({
                        ...normalizeGatewaySettings(prev),
                        read_timeout_secs: parseInt(e.target.value, 10) || 5,
                      }))
                    }
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayReadTimeoutDesc", "Wait time for headers & stream chunks (5-3600s).")}
                  </p>
                </label>

                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayRequestBudgetLabel", "Request Budget (s)")}
                  </span>
                  <input
                    type="number"
                    min={10}
                    max={7200}
                    value={safeSettings.request_budget_secs}
                    onChange={(e) =>
                      setGatewaySettings?.((prev) => ({
                        ...normalizeGatewaySettings(prev),
                        request_budget_secs: parseInt(e.target.value, 10) || 10,
                      }))
                    }
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayRequestBudgetDesc", "Total maximum client wait budget (10-7200s).")}
                  </p>
                </label>
              </div>

              <hr className="border-border/50" />

              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayMaxRetriesLabel", "Max Retries Per Provider")}
                  </span>
                  <input
                    type="number"
                    min={0}
                    max={20}
                    value={safeSettings.max_retries_per_provider}
                    onChange={(e) =>
                      setGatewaySettings?.((prev) => ({
                        ...normalizeGatewaySettings(prev),
                        max_retries_per_provider: parseInt(e.target.value, 10) || 0,
                      }))
                    }
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayMaxRetriesDesc", "Max retries attempted on same provider (0-20).")}
                  </p>
                </label>

                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayRetryBaseDelayLabel", "Retry Base Delay (ms)")}
                  </span>
                  <input
                    type="number"
                    min={100}
                    max={30000}
                    step={100}
                    value={safeSettings.retry_base_delay_ms}
                    onChange={(e) =>
                      setGatewaySettings?.((prev) => ({
                        ...normalizeGatewaySettings(prev),
                        retry_base_delay_ms: parseInt(e.target.value, 10) || 100,
                      }))
                    }
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayRetryBaseDelayDesc", "Initial backoff delay before retrying (100-30000ms).")}
                  </p>
                </label>
              </div>
            </div>
          </SettingCard>

          {/* Group 3: Resilience & Circuit Breaking */}
          <SettingCard
            id="section-ai-gateway-resilience"
            icon={ShieldCheck}
            title={t("aiGatewaySectionResilience", "Fault Tolerance & Circuit Breaking")}
            description={t(
              "aiGatewaySectionResilienceDesc",
              "Configure circuit breaker failure thresholds, probe cooldown, and quota TTL recovery.",
            )}
          >
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <label className="block space-y-1.5">
                <span className="text-sm font-medium">
                  {t("aiGatewayFailureThresholdLabel", "Failure Threshold")}
                </span>
                <input
                  type="number"
                  min={0}
                  max={50}
                  value={safeSettings.failure_threshold}
                  onChange={(e) =>
                    setGatewaySettings?.((prev) => ({
                      ...normalizeGatewaySettings(prev),
                      failure_threshold: parseInt(e.target.value, 10) || 0,
                    }))
                  }
                  className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                <p className="text-xs text-muted-foreground">
                  {t("aiGatewayFailureThresholdDesc", "Consecutive failures to trigger circuit breaker (0 to disable).")}
                </p>
              </label>

              <label className="block space-y-1.5">
                <span className="text-sm font-medium">
                  {t("aiGatewayProbeCooldownLabel", "Probe Cooldown (s)")}
                </span>
                <input
                  type="number"
                  min={5}
                  max={7200}
                  value={safeSettings.probe_cooldown_secs}
                  onChange={(e) =>
                    setGatewaySettings?.((prev) => ({
                      ...normalizeGatewaySettings(prev),
                      probe_cooldown_secs: parseInt(e.target.value, 10) || 5,
                    }))
                  }
                  className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                <p className="text-xs text-muted-foreground">
                  {t("aiGatewayProbeCooldownDesc", "Cooldown before half-open probe attempt (5-7200s).")}
                </p>
              </label>

              <label className="block space-y-1.5">
                <span className="text-sm font-medium">
                  {t("aiGatewayKeyQuotaTtlLabel", "Key Quota TTL (s)")}
                </span>
                <input
                  type="number"
                  min={30}
                  max={86400}
                  value={safeSettings.key_quota_ttl_secs}
                  onChange={(e) =>
                    setGatewaySettings?.((prev) => ({
                      ...normalizeGatewaySettings(prev),
                      key_quota_ttl_secs: parseInt(e.target.value, 10) || 30,
                    }))
                  }
                  className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                <p className="text-xs text-muted-foreground">
                  {t("aiGatewayKeyQuotaTtlDesc", "Auto-recovery TTL after key 429 quota exhaustion (30-86400s).")}
                </p>
              </label>
            </div>
          </SettingCard>

          {/* Group 4: Session & Context Affinity */}
          <SettingCard
            id="section-ai-gateway-affinity"
            icon={Route}
            title={t("aiGatewaySectionAffinity", "Session & Context Affinity")}
            description={t(
              "aiGatewaySectionAffinityDesc",
              "Manage conversation sticky routing, model pool context hold, and custom header inspection.",
            )}
          >
            <div className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewaySessionAffinityTimeoutLabel", "Session Affinity Timeout (s)")}
                  </span>
                  <input
                    type="number"
                    min={0}
                    max={86400}
                    value={safeSettings.session_affinity_timeout_secs}
                    onChange={(e) =>
                      setGatewaySettings?.((prev) => ({
                        ...normalizeGatewaySettings(prev),
                        session_affinity_timeout_secs: parseInt(e.target.value, 10) || 0,
                      }))
                    }
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewaySessionAffinityTimeoutDesc", "Idle timeout for conversation stickiness (0 to disable).")}
                  </p>
                </label>

                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayContextHoldTimeoutLabel", "Context Hold Timeout (s)")}
                  </span>
                  <input
                    type="number"
                    min={0}
                    max={86400}
                    value={safeSettings.context_hold_timeout_secs}
                    onChange={(e) =>
                      setGatewaySettings?.((prev) => ({
                        ...normalizeGatewaySettings(prev),
                        context_hold_timeout_secs: parseInt(e.target.value, 10) || 0,
                      }))
                    }
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayContextHoldTimeoutDesc", "Duration to hold model pool multi-turn context (0 to disable).")}
                  </p>
                </label>
              </div>

              <hr className="border-border/50" />

              <label className="block space-y-1.5">
                <span className="text-sm font-medium">
                  {t("aiGatewayCustomSessionHeadersLabel", "Custom Session Headers")}
                </span>
                <input
                  type="text"
                  placeholder="x-session-id, x-trace-id, x-conversation-id"
                  value={(safeSettings.custom_session_headers || []).join(", ")}
                  onChange={(e) => {
                    const headers = e.target.value
                      .split(",")
                      .map((h) => h.trim())
                      .filter(Boolean);
                    setGatewaySettings?.((prev) => ({
                      ...normalizeGatewaySettings(prev),
                      custom_session_headers: headers,
                    }));
                  }}
                  className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                <p className="text-xs text-muted-foreground">
                  {t("aiGatewayCustomSessionHeadersDesc", "Comma-separated HTTP headers used to identify conversational sessions.")}
                </p>
              </label>
            </div>
          </SettingCard>

          {/* Group 5: Maintenance & Quota */}
          <SettingCard
            id="section-ai-gateway-maintenance"
            icon={RefreshCw}
            title={t("aiGatewaySectionMaintenance", "Maintenance & Quota Strategy")}
            description={t(
              "aiGatewaySectionMaintenanceDesc",
              "Configure daily quota reset timezone, quota caching, request logs, and template sync intervals.",
            )}
          >
            <div className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayDailyResetOffsetLabel", "Daily Reset Timezone (Hours)")}
                  </span>
                  <input
                    type="number"
                    min={-12}
                    max={14}
                    value={safeSettings.daily_reset_offset_hours}
                    onChange={(e) =>
                      setGatewaySettings?.((prev) => ({
                        ...normalizeGatewaySettings(prev),
                        daily_reset_offset_hours: parseInt(e.target.value, 10) || 0,
                      }))
                    }
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayDailyResetOffsetDesc", "Offset for daily reset calculation (UTC+8 is 8, UTC is 0, -12 to 14).")}
                  </p>
                </label>

                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayQuotaCacheTtlLabel", "Quota Cache TTL (s)")}
                  </span>
                  <input
                    type="number"
                    min={10}
                    max={3600}
                    value={safeSettings.quota_cache_ttl_secs}
                    onChange={(e) =>
                      setGatewaySettings?.((prev) => ({
                        ...normalizeGatewaySettings(prev),
                        quota_cache_ttl_secs: parseInt(e.target.value, 10) || 10,
                      }))
                    }
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayQuotaCacheTtlDesc", "Cache lifetime for upstream provider quota queries (10-3600s).")}
                  </p>
                </label>
              </div>

              <hr className="border-border/50" />

              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayRetentionLabel", "Log retention days")}
                  </span>
                  <input
                    type="number"
                    min={1}
                    max={365}
                    value={usageRetentionInput}
                    onChange={(event) => setUsageRetentionInput(event.target.value)}
                    aria-label={t("aiGatewayRetentionLabel", "Log retention days")}
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayRetentionDesc", "Request logs older than this many days are permanently deleted.")}
                  </p>
                </label>

                <label className="block space-y-1.5">
                  <span className="text-sm font-medium">
                    {t("aiGatewayTemplateAutoRefreshLabel", "Template auto refresh minutes")}
                  </span>
                  <input
                    type="number"
                    min={0}
                    max={1440}
                    value={templateAutoRefreshInput}
                    onChange={(event) => setTemplateAutoRefreshInput(event.target.value)}
                    aria-label={t("aiGatewayTemplateAutoRefreshLabel", "Template auto refresh minutes")}
                    className="w-full rounded-xl border bg-background px-4 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  />
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayTemplateAutoRefreshDesc", "Periodic interval to refresh model definitions (0 to disable).")}
                  </p>
                </label>
              </div>

              <hr className="border-border/50" />

              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <h4 className="text-sm font-medium text-foreground">
                    {t("aiGatewayDebugLoggingLabel", "Debug Logging")}
                  </h4>
                  <p className="text-xs text-muted-foreground">
                    {t("aiGatewayDebugLoggingDesc", "Enable verbose diagnostic logs in backend console.")}
                  </p>
                </div>
                <Switch
                  checked={safeSettings.debug_logging}
                  onCheckedChange={(checked) =>
                    setGatewaySettings?.((prev) => ({
                      ...normalizeGatewaySettings(prev),
                      debug_logging: checked,
                    }))
                  }
                />
              </div>
            </div>
          </SettingCard>
        </div>
      )}

      {/* 4. SSL Certificate */}
      {subTab === "ssl-certificate" && (
        <div className="space-y-6">
          <SettingCard
            id="section-ssl-certificate"
            icon={ShieldCheck}
            title={t(
              "sslCertificateSettingsTitle",
              "SSL 证书巡检与告警设置",
            )}
            description={t(
              "sslCertificateSettingsDesc",
              "配置域名 SSL 证书的每日自动巡检时间节点与临期告警阈值。",
            )}
          >
            {/* Auto check switch */}
            <div className="flex items-center justify-between">
              <div>
                <h4 className="text-sm font-semibold text-foreground">
                  {t("sslAutoCheckEnabled", "启用每日自动巡检")}
                </h4>
                <p className="text-xs text-muted-foreground mt-0.5">
                  {t(
                    "sslAutoCheckEnabledDesc",
                    "开启后，系统将在设定的时间节点自动对关注列表中的域名进行 TLS 握手探测。",
                  )}
                </p>
              </div>
              <Switch
                checked={sslConfig.autoCheckEnabled}
                onCheckedChange={(checked) =>
                  setSslConfig((prev) => ({
                    ...prev,
                    autoCheckEnabled: checked,
                  }))
                }
              />
            </div>

            <hr className="border-border/50" />

            {/* Check times */}
            <div className="space-y-3">
              <div>
                <h4 className="text-sm font-semibold text-foreground">
                  {t("sslCheckTimesTitle", "每日自动巡检时间节点")}
                </h4>
                <p className="text-xs text-muted-foreground mt-0.5">
                  {t(
                    "sslCheckTimesDesc",
                    "支持配置多个 24 小时制“时:分”时间节点（例如 09:00、18:00）。",
                  )}
                </p>
              </div>

              <div className="flex flex-wrap items-center gap-2">
                {sslConfig.checkTimes.map((timeStr) => (
                  <span
                    key={timeStr}
                    className="inline-flex items-center gap-1.5 rounded-lg border bg-muted/60 px-3 py-1 text-xs font-mono font-medium text-foreground"
                  >
                    <Clock className="w-3.5 h-3.5 text-muted-foreground" />
                    {timeStr}
                    {sslConfig.checkTimes.length > 1 && (
                      <button
                        type="button"
                        onClick={() =>
                          setSslConfig((prev) => ({
                            ...prev,
                            checkTimes: prev.checkTimes.filter(
                              (t) => t !== timeStr,
                            ),
                          }))
                        }
                        className="ml-1 rounded p-0.5 hover:bg-muted text-muted-foreground hover:text-rose-500 transition-colors"
                        title={t("delete", "删除")}
                      >
                        <X className="w-3 h-3" />
                      </button>
                    )}
                  </span>
                ))}
              </div>

              <div className="flex items-center gap-2 pt-1">
                <input
                  type="time"
                  value={newCheckTimeInput}
                  onChange={(e) => setNewCheckTimeInput(e.target.value)}
                  className="rounded-lg border bg-background px-3 py-1.5 text-xs font-mono text-foreground focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                <button
                  type="button"
                  onClick={() => {
                    const trimmed = newCheckTimeInput.trim();
                    if (!/^([01]\d|2[0-3]):([0-5]\d)$/.test(trimmed)) {
                      onShowToast?.(
                        "error",
                        t("invalidTimeFormat", "时间格式无效，请输入 HH:mm 格式"),
                      );
                      return;
                    }
                    if (sslConfig.checkTimes.includes(trimmed)) {
                      onShowToast?.("error", t("duplicateTimeNode", "该时间节点已存在"));
                      return;
                    }
                    const updated = [...sslConfig.checkTimes, trimmed].sort();
                    setSslConfig((prev) => ({
                      ...prev,
                      checkTimes: updated,
                    }));
                  }}
                  className="inline-flex items-center gap-1 rounded-lg border bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-muted transition-colors"
                >
                  <Plus className="w-3.5 h-3.5" />
                  <span>{t("addCheckTime", "添加时间节点")}</span>
                </button>
              </div>
            </div>

            <hr className="border-border/50" />

            {/* Alert days threshold */}
            <div className="space-y-3">
              <div>
                <h4 className="text-sm font-semibold text-foreground">
                  {t("sslAlertThresholdTitle", "临期告警阈值（天）")}
                </h4>
                <p className="text-xs text-muted-foreground mt-0.5">
                  {t(
                    "sslAlertThresholdDesc",
                    "当证书剩余天数小于等于该天数时，自动标记为临期并生成消息中心通知。",
                  )}
                </p>
              </div>

              <div className="flex items-center gap-3">
                <input
                  type="number"
                  min={1}
                  max={365}
                  value={sslAlertDaysInput}
                  onChange={(e) => setSslAlertDaysInput(e.target.value)}
                  className="w-32 rounded-xl border bg-background px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                />
                <span className="text-xs text-muted-foreground">天</span>
              </div>

              <div className="rounded-xl bg-amber-500/10 border border-amber-500/20 p-3.5 text-xs text-amber-700 dark:text-amber-400">
                <p className="font-semibold">
                  {t("sslDailyReminderNotice", "⚡ 每日提醒机制说明：")}
                </p>
                <p className="mt-1 text-[11px] leading-relaxed">
                  {t(
                    "sslDailyReminderNoticeDetail",
                    "当证书剩余天数 <= 7 天且未续期时，系统每天都会在自动巡检后提醒并加入消息中心，直到证书续期恢复正常。",
                  )}
                </p>
              </div>
            </div>
          </SettingCard>
        </div>
      )}
    </div>
  );
}
