import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  CheckCircle2,
  Clock,
  RefreshCw,
  TerminalSquare,
  Wand2,
} from "lucide-react";
import {
  API_FUSION_SUPPORTED_TERMINAL_TOOLS,
  resolveDefaultKeyId,
  type FusionConfig,
  type FusionTerminalTarget,
} from "@/lib/apiFusion";

type TerminalSyncPanelProps = {
  targets: FusionTerminalTarget[];
  config: FusionConfig;
  busy: boolean;
  onConfigureTool: (tool: string) => void;
  onSyncTool: (tool: string) => void;
};

export function TerminalSyncPanel({
  targets,
  config,
  busy,
  onConfigureTool,
  onSyncTool,
}: TerminalSyncPanelProps) {
  const { t } = useTranslation();
  const defaultKeyId = resolveDefaultKeyId(config.keys, config.default_key_id);
  const defaultKeyMissing = !defaultKeyId;
  const supportedTargets = targets.filter((target) =>
    API_FUSION_SUPPORTED_TERMINAL_TOOLS.includes(
      target.tool as (typeof API_FUSION_SUPPORTED_TERMINAL_TOOLS)[number],
    ),
  );
  const actionsDisabled = busy || defaultKeyMissing;

  return (
    <section className="space-y-3.5" data-testid="api-fusion-terminals">
      {/* 头部：标题与说明 */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2">
            <TerminalSquare className="h-4 w-4 text-indigo-600" />
            <h3 className="text-sm font-semibold text-foreground">
              {t("apiFusionTerminalSync", "Terminal sync")}
            </h3>
            <span className="rounded-full bg-muted px-2 py-0.2 text-[10px] font-semibold text-muted-foreground">
              {supportedTargets.length}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            {t(
              "apiFusionTerminalSyncDesc",
              "Each tool shows Add or Sync on its right based on its state. Syncing writes the gateway endpoint, default key, and model mappings as an independent provider, and it is not activated automatically. If its gateway provider was deleted, syncing creates a new API Gateway provider.",
            )}
          </p>
        </div>
      </div>

      {/* 缺失默认 Key 提示 */}
      {defaultKeyMissing ? (
        <div
          className="flex items-start gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 p-2.5 text-xs text-amber-700 dark:text-amber-400"
          data-testid="api-fusion-default-key-required"
          role="alert"
        >
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <div>
            <div className="font-semibold text-xs">
              {t("apiFusionDefaultKeyRequiredTitle", "Local default key required")}
            </div>
            <div className="mt-0.5 text-[11px]">
              {t(
                "apiFusionDefaultKeyRequired",
                "Add and enable a local key before configuring terminals.",
              )}
            </div>
          </div>
        </div>
      ) : null}

      {/* 目标列表 */}
      {supportedTargets.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/50 px-6 py-10 text-center">
          <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
            <TerminalSquare className="h-5 w-5" />
          </div>
          <h4 className="mt-2.5 text-xs font-medium text-foreground">
            {t("apiFusionNoTerminalTargets", "No OpenCode or Codex terminal targets found.")}
          </h4>
          <p className="mt-1 max-w-sm text-xs text-muted-foreground">
            {t(
              "apiFusionNoTerminalTargetsGuide",
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

            return (
              <div
                key={target.tool}
                data-testid={`api-fusion-target-${target.tool}`}
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
                  <div className="mt-0.5 truncate font-mono text-xs text-muted-foreground">
                    {target.base_url ?? t("apiFusionNoValue", "not set")}
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <span
                    className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium leading-4 ${
                      pending
                        ? "bg-amber-500/10 text-amber-700 dark:text-amber-400"
                        : "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                    }`}
                    data-testid={`api-fusion-target-status-${target.tool}`}
                  >
                    {pending ? (
                      <>
                        <Clock className="h-3 w-3" />
                        {t("apiFusionPendingSync", "Pending sync")}
                      </>
                    ) : (
                      <>
                        <CheckCircle2 className="h-3 w-3" />
                        {t("apiFusionSynced", "Synced")}
                      </>
                    )}
                  </span>
                  {hasBeenAdded ? (
                    <button
                      type="button"
                      onClick={() => onSyncTool(target.tool)}
                      disabled={actionsDisabled}
                      className="inline-flex h-7 items-center gap-1.5 rounded-lg bg-primary px-2.5 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
                    >
                      <RefreshCw className="h-3.5 w-3.5" />
                      {t("apiFusionSyncOne", "Sync")}
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => onConfigureTool(target.tool)}
                      disabled={actionsDisabled}
                      className="inline-flex h-7 items-center gap-1.5 rounded-lg border bg-background px-2.5 text-xs font-medium shadow-sm transition hover:bg-muted disabled:opacity-50"
                    >
                      <Wand2 className="h-3.5 w-3.5" />
                      {t("apiFusionConfigureSelected", "Add provider")}
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
