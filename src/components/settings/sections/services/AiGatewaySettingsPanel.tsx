import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import { Network, Route, Clock, ShieldCheck, RefreshCw } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { SettingCard } from "../../common/SettingCard";
import { SettingSelect } from "../../common/SettingSelect";
import {
  normalizeGatewaySettings,
  type GatewaySettings,
} from "@/lib/aiGateway";

interface AiGatewaySettingsPanelProps {
  t: TFunction;
  gatewaySettings: GatewaySettings;
  setGatewaySettings?: Dispatch<SetStateAction<GatewaySettings>>;
  gatewayPortInput: string;
  setGatewayPortInput?: (val: string) => void;
  usageRetentionInput: string;
  setUsageRetentionInput: (val: string) => void;
  templateAutoRefreshInput: string;
  setTemplateAutoRefreshInput: (val: string) => void;
}

export function AiGatewaySettingsPanel({
  t,
  gatewaySettings,
  setGatewaySettings,
  gatewayPortInput,
  setGatewayPortInput,
  usageRetentionInput,
  setUsageRetentionInput,
  templateAutoRefreshInput,
  setTemplateAutoRefreshInput,
}: AiGatewaySettingsPanelProps) {
  const safeSettings = normalizeGatewaySettings(gatewaySettings);

  return (
    <div className="space-y-6">
      {/* Group 1: Network & Access */}
      <SettingCard
        id="section-ai-gateway-network"
        icon={Network}
        title={t("aiGatewaySectionNetwork", "Network & Access Control")}
        description={t(
          "aiGatewaySectionNetworkDesc",
          "Configure listening port, network binding, LAN security, and CORS.",
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
  );
}
