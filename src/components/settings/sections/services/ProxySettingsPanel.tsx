import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import { Globe, CheckCircle2, AlertCircle, RefreshCw } from "lucide-react";
import { SettingCard } from "../../common/SettingCard";
import type { ProxyConfig, ProxyStatus } from "../../types";

interface ProxySettingsPanelProps {
  t: TFunction;
  proxyConfig: ProxyConfig;
  setProxyConfig: Dispatch<SetStateAction<ProxyConfig>>;
  proxyStatus: ProxyStatus | null;
  testingProxy: boolean;
  onTestProxy?: () => Promise<void>;
  authEnabled: boolean;
  setAuthEnabled: (enabled: boolean) => void;
}

export function ProxySettingsPanel({
  t,
  proxyConfig,
  setProxyConfig,
  proxyStatus,
  testingProxy,
  onTestProxy,
  authEnabled,
  setAuthEnabled,
}: ProxySettingsPanelProps) {
  return (
    <SettingCard
      id="section-proxy"
      icon={Globe}
      title={t("proxySettings", "Network Proxy Settings")}
      description={t(
        "proxySettingsDesc",
        "One global proxy for application-managed HTTP(S) requests such as AI Gateway forwarding, model lists, MCP, updates, SSL/ACME, mail and OAuth.",
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
              "Saved changes apply to the next admitted request or retry without restarting OneSpace.",
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

      <p className="text-xs text-muted-foreground leading-relaxed">
        {t(
          "proxyScopeNotice",
          "Loopback destinations (localhost, 127.0.0.1, ::1) always connect directly. External terminals, browsers and already running processes are not retroactively changed.",
        )}
      </p>

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
  );
}
