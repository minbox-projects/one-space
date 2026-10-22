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
        <SettingCard
          id="section-ai-gateway"
          icon={Network}
          title={t("aiGatewaySettings", "AI Gateway")}
          description={t(
            "aiGatewaySettingsDesc",
            "Configure how long AI Gateway request logs are retained. Prices and usage live in the AI Gateway workspace.",
          )}
        >
          <div className="space-y-4">
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
                className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
              />
            </label>
            <p className="text-xs text-muted-foreground">
              {t(
                "aiGatewayRetentionDesc",
                "Request logs older than this many days are permanently deleted when new logs are written.",
              )}
            </p>

            <hr className="border-border/50" />

            <label className="block space-y-1.5">
              <span className="text-sm font-medium">
                {t(
                  "aiGatewayTemplateAutoRefreshLabel",
                  "Template auto refresh minutes",
                )}
              </span>
              <input
                type="number"
                min={0}
                max={1440}
                value={templateAutoRefreshInput}
                onChange={(event) =>
                  setTemplateAutoRefreshInput(event.target.value)
                }
                aria-label={t(
                  "aiGatewayTemplateAutoRefreshLabel",
                  "Template auto refresh minutes",
                )}
                className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
              />
            </label>
            <p className="text-xs text-muted-foreground">
              {t(
                "aiGatewayTemplateAutoRefreshDesc",
                "Periodic interval to refresh model pricing and definitions (0 to disable).",
              )}
            </p>
          </div>
        </SettingCard>
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
