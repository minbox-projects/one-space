import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  Clock,
  HelpCircle,
  Play,
  RefreshCw,
  TerminalSquare,
  Wand2,
} from "lucide-react";
import {
  API_GATEWAY_SUPPORTED_TERMINAL_TOOLS,
  formatGatewayTimestamp,
  resolveDefaultKeyId,
  type GatewayConfig,
  type GatewayTerminalTarget,
} from "@/lib/apiGateway";

type TerminalSyncPanelProps = {
  targets: GatewayTerminalTarget[];
  config: GatewayConfig;
  syncingTools: Record<string, boolean>;
  gatewayRunning?: boolean;
  onStartGateway?: () => void;
  startingGateway?: boolean;
  onConfigureTool: (tool: string) => void;
  onSyncTool: (tool: string) => void;
};

export function TerminalSyncPanel({
  targets,
  config,
  syncingTools,
  gatewayRunning,
  onStartGateway,
  startingGateway,
  onConfigureTool,
  onSyncTool,
}: TerminalSyncPanelProps) {
  const { t } = useTranslation();
  const [showFaq, setShowFaq] = useState(false);
  const defaultKeyId = resolveDefaultKeyId(config.keys, config.default_key_id);
  const defaultKeyMissing = !defaultKeyId;
  const supportedTargets = targets.filter((target) =>
    API_GATEWAY_SUPPORTED_TERMINAL_TOOLS.includes(
      target.tool as (typeof API_GATEWAY_SUPPORTED_TERMINAL_TOOLS)[number],
    ),
  );

  return (
    <div className="space-y-3.5">
      {/* 头部：标题与说明 */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2">
            <TerminalSquare className="h-4 w-4 text-primary" />
            <h3 className="text-sm font-semibold text-foreground">
              {t("apiGatewayTerminalSync", "AI terminal integration")}
            </h3>
            <span className="rounded-full bg-muted px-2 py-0.2 text-[10px] font-semibold text-muted-foreground">
              {supportedTargets.length}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            {t(
              "apiGatewayTerminalSyncDesc",
              "Each tool shows Add or Sync on its right based on its state. Syncing writes the gateway endpoint, default key, and model mappings as an independent provider, and it is not activated automatically. If its gateway provider was deleted, syncing creates a new API Gateway provider.",
            )}
          </p>
        </div>
      </div>

      {/* 缺失默认 Key 提示 */}
      {defaultKeyMissing ? (
        <div
          className="flex items-start gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 p-2.5 text-xs text-amber-700 dark:text-amber-400"
          data-testid="api-gateway-default-key-required"
          role="alert"
        >
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <div>
            <div className="font-semibold text-xs">
              {t("apiGatewayDefaultKeyRequiredTitle", "Local default key required")}
            </div>
            <div className="mt-0.5 text-[11px]">
              {t(
                "apiGatewayDefaultKeyRequired",
                "Add and enable a local key before configuring terminals.",
              )}
            </div>
          </div>
        </div>
      ) : null}

      {/* 网关未启动即时告警 */}
      {gatewayRunning === false ? (
        <div
          className="flex flex-col gap-2 rounded-xl border border-amber-500/30 bg-amber-500/10 p-3 text-xs text-amber-800 dark:text-amber-300"
          data-testid="api-gateway-terminal-stopped-warning"
          role="alert"
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-start gap-2.5 min-w-0 flex-1">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600 dark:text-amber-400" />
              <div>
                <div className="font-semibold text-xs text-amber-900 dark:text-amber-200">
                  {t("apiGatewayTerminalStoppedTitle", "本地 API 网关服务未启动")}
                </div>
                <div className="mt-0.5 text-[11px] leading-relaxed text-amber-800/90 dark:text-amber-300/90">
                  {t(
                    "apiGatewayTerminalStoppedDesc",
                    "当前网关处于停止状态。在终端调用已配置的工具时，将报错：Cannot connect to API: Unable to connect. Is the computer able to access the url? 请先启动网关保持运行。",
                  )}
                </div>
              </div>
            </div>
            {onStartGateway ? (
              <button
                type="button"
                onClick={onStartGateway}
                disabled={startingGateway}
                data-testid="api-gateway-start-from-terminals"
                className="shrink-0 inline-flex h-7 items-center gap-1.5 rounded-lg border border-emerald-500/40 bg-emerald-500/15 px-2.5 text-xs font-medium text-emerald-700 hover:bg-emerald-500/25 active:bg-emerald-500/30 dark:text-emerald-300 transition shadow-xs disabled:opacity-50"
              >
                {startingGateway ? (
                  <RefreshCw className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Play className="h-3 w-3 fill-current translate-x-0.2" />
                )}
                <span>{t("apiGatewayStartServiceNow", "立即启动服务")}</span>
              </button>
            ) : null}
          </div>
        </div>
      ) : null}

      {/* 目标列表 */}
      <section data-testid="api-gateway-terminals">
        {supportedTargets.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/50 px-6 py-10 text-center">
          <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
            <TerminalSquare className="h-5 w-5" />
          </div>
          <h4 className="mt-2.5 text-xs font-medium text-foreground">
            {t("apiGatewayNoTerminalTargets", "No OpenCode or Codex terminal targets found.")}
          </h4>
          <p className="mt-1 max-w-sm text-xs text-muted-foreground">
            {t(
              "apiGatewayNoTerminalTargetsGuide",
              "Install or configure OpenCode / Codex providers first to enable automatic endpoint synchronization.",
            )}
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {supportedTargets.map((target) => {
            const pending = target.pending_sync;
            // The row action follows whether this tool was ever added to the
            // gateway, not whether the current sync is up to date. A stale
            // ledger (synced=false with synced_key_id/synced_at still set)
            // means the backend provider was removed and Sync must recreate it.
            const hasBeenAdded =
              target.synced ||
              target.synced_key_id !== null ||
              target.synced_at !== null;
            const isSyncing = Boolean(syncingTools[target.tool]);
            const rowDisabled = isSyncing || defaultKeyMissing;

            return (
              <div
                key={target.tool}
                data-testid={`api-gateway-target-${target.tool}`}
                className="flex flex-wrap items-center justify-between gap-3 rounded-xl border bg-card p-3 shadow-sm transition hover:border-primary/40"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-semibold leading-5 text-foreground">
                      {target.name}
                    </span>
                    <span className="rounded border bg-background px-1.5 py-0.2 font-mono text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                      {target.tool}
                    </span>
                  </div>
                  <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground">
                    <span className="truncate font-mono">
                      {target.base_url ?? t("apiGatewayNoValue", "not set")}
                    </span>
                    <span className="text-muted-foreground/30">•</span>
                    <span
                      data-testid={`api-gateway-target-synced-${target.tool}`}
                      className="inline-flex items-center gap-1 text-[11px]"
                    >
                      <span className="shrink-0">
                        {t("apiGatewayTerminalLastSync", "Last sync")}:
                      </span>
                      <span className="font-medium text-foreground/80">
                        {target.synced_at
                          ? formatGatewayTimestamp(target.synced_at)
                          : t("apiGatewayTerminalNotSynced", "Not synced yet")}
                      </span>
                    </span>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <span
                    className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium leading-4 ${
                      pending
                        ? "bg-amber-500/10 text-amber-700 dark:text-amber-400"
                        : "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                    }`}
                    data-testid={`api-gateway-target-status-${target.tool}`}
                  >
                    {pending ? (
                      <>
                        <Clock className="h-3 w-3" />
                        {t("apiGatewayPendingSync", "Pending sync")}
                      </>
                    ) : (
                      <>
                        <CheckCircle2 className="h-3 w-3" />
                        {t("apiGatewaySynced", "Synced")}
                      </>
                    )}
                  </span>
                  {hasBeenAdded ? (
                    <button
                      type="button"
                      onClick={() => onSyncTool(target.tool)}
                      disabled={rowDisabled}
                      data-testid={`api-gateway-sync-${target.tool}`}
                      className="inline-flex h-7 items-center gap-1.5 rounded-lg bg-primary px-2.5 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
                    >
                      <RefreshCw
                        className={`h-3.5 w-3.5 ${isSyncing ? "animate-spin" : ""}`}
                      />
                      {t("apiGatewaySyncOne", "Sync")}
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => onConfigureTool(target.tool)}
                      disabled={rowDisabled}
                      data-testid={`api-gateway-sync-${target.tool}`}
                      className="inline-flex h-7 items-center gap-1.5 rounded-lg border bg-background px-2.5 text-xs font-medium shadow-sm transition hover:bg-muted disabled:opacity-50"
                    >
                      <Wand2
                        className={`h-3.5 w-3.5 ${isSyncing ? "animate-spin" : ""}`}
                      />
                      {t("apiGatewayConfigureSelected", "Add provider")}
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
      </section>

      {/* 终端连接常见问题排查指引 */}
      <div className="rounded-xl border bg-muted/20 p-3 text-xs" data-testid="api-gateway-terminal-faq">
        <button
          type="button"
          onClick={() => setShowFaq((prev) => !prev)}
          className="flex w-full items-center justify-between font-semibold text-foreground hover:text-primary transition"
          aria-expanded={showFaq}
          data-testid="api-gateway-terminal-faq-toggle"
        >
          <div className="flex items-center gap-2">
            <HelpCircle className="h-3.5 w-3.5 text-muted-foreground" />
            <span>{t("apiGatewayTerminalFaqTitle", "终端调用常见报错排查")}</span>
          </div>
          <ChevronDown
            className={`h-3.5 w-3.5 text-muted-foreground transition-transform duration-200 ${
              showFaq ? "rotate-180" : ""
            }`}
          />
        </button>
        {showFaq ? (
          <div
            className="mt-2.5 space-y-2.5 pt-2.5 border-t border-border/50 text-[11px] text-muted-foreground leading-relaxed animate-in fade-in-0 duration-200"
            data-testid="api-gateway-terminal-faq-content"
          >
            <div className="space-y-0.5">
              <div className="font-medium text-foreground">
                Q: {t("apiGatewayFaqQ1", "终端提示 Cannot connect to API: Unable to connect. Is the computer able to access the url?")}
              </div>
              <p>
                {t(
                  "apiGatewayFaqA1",
                  "该错误表示终端工具无法连接到本地网关地址（127.0.0.1:{{port}}）。排查步骤：1. 确认顶部网关服务处于“运行中”；2. 检查 {{port}} 端口是否冲突；3. 确保本地密钥列表中有已启用的密钥。",
                  { port: config.port },
                )}
              </p>
            </div>
            <div className="space-y-0.5">
              <div className="font-medium text-foreground">
                Q: {t("apiGatewayFaqQ2", "终端提示 all providers unavailable: 429 或额度用尽？")}
              </div>
              <p>
                {t(
                  "apiGatewayFaqA2",
                  "表示所有能处理该请求的上游服务商均触发限流或额度已用尽。排查步骤：1. 检查上游服务商账户余额/周期配额；2. 在“上游服务商”列表配置备用服务商以实现自动故障转移。",
                )}
              </p>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}
