import { useTranslation } from "react-i18next";
import { AlertTriangle, RefreshCw, Wand2 } from "lucide-react";
import {
  API_FUSION_SUPPORTED_TERMINAL_TOOLS,
  isTerminalSyncPending,
  resolveDefaultKeyId,
  type FusionConfig,
  type FusionTerminalTarget,
} from "@/lib/apiFusion";

type TerminalSyncPanelProps = {
  targets: FusionTerminalTarget[];
  config: FusionConfig;
  selectedTargetIds: string[];
  busy: boolean;
  onToggleTarget: (providerId: string) => void;
  onConfigure: () => void;
  onSync: () => void;
};

export function TerminalSyncPanel({
  targets,
  config,
  selectedTargetIds,
  busy,
  onToggleTarget,
  onConfigure,
  onSync,
}: TerminalSyncPanelProps) {
  const { t } = useTranslation();
  const defaultKeyId = resolveDefaultKeyId(config.keys, config.default_key_id);
  const defaultKeyMissing = !defaultKeyId;
  const supportedTargets = targets.filter((target) =>
    API_FUSION_SUPPORTED_TERMINAL_TOOLS.includes(
      target.tool as (typeof API_FUSION_SUPPORTED_TERMINAL_TOOLS)[number],
    ),
  );
  const hasSelection = selectedTargetIds.length > 0;
  const actionsDisabled = busy || defaultKeyMissing || !hasSelection;

  return (
    <section className="rounded-[24px] border bg-card p-5" data-testid="api-fusion-terminals">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold">{t("apiFusionTerminalSync", "Terminal sync")}</h3>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onConfigure}
            disabled={actionsDisabled}
            className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-sm transition hover:bg-muted disabled:opacity-50"
          >
            <Wand2 className="h-4 w-4" />
            {t("apiFusionConfigureSelected", "Configure selected")}
          </button>
          <button
            type="button"
            onClick={onSync}
            disabled={actionsDisabled}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-50"
          >
            <RefreshCw className="h-4 w-4" />
            {t("apiFusionSyncSelected", "Sync selected")}
          </button>
        </div>
      </div>

      <p className="mt-2 text-xs text-muted-foreground">
        {t(
          "apiFusionTerminalSyncDesc",
          "Write the local API address and default local key to the selected OpenCode / Codex records. Only base_url and api_key change.",
        )}
      </p>

      {defaultKeyMissing ? (
        <p
          className="mt-3 flex items-start gap-2 rounded-xl border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-xs text-amber-700"
          data-testid="api-fusion-default-key-required"
          role="alert"
        >
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          {t(
            "apiFusionDefaultKeyRequired",
            "Add and enable a local key before configuring terminals.",
          )}
        </p>
      ) : null}

      {supportedTargets.length === 0 ? (
        <p className="mt-4 rounded-2xl border border-dashed px-4 py-5 text-sm text-muted-foreground">
          {t("apiFusionNoTerminalTargets", "No OpenCode or Codex terminal targets found.")}
        </p>
      ) : (
        <ul className="mt-4 space-y-2">
          {supportedTargets.map((target) => {
            const pending = isTerminalSyncPending(target, config);
            const checked = selectedTargetIds.includes(target.provider_id);
            return (
              <li
                key={target.provider_id}
                data-testid={`api-fusion-target-${target.provider_id}`}
                className="flex flex-wrap items-center gap-3 rounded-2xl border bg-muted/10 px-4 py-3"
              >
                <label className="inline-flex min-w-0 flex-1 items-center gap-3">
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={() => onToggleTarget(target.provider_id)}
                    aria-label={target.name}
                    className="h-4 w-4"
                  />
                  <span className="min-w-0">
                    <span className="block truncate text-sm font-medium">{target.name}</span>
                    <span className="block truncate font-mono text-xs text-muted-foreground">
                      {target.tool} · {target.base_url ?? t("apiFusionNoValue", "not set")}
                    </span>
                  </span>
                </label>
                <span
                  className={`rounded-full px-2.5 py-1 text-[11px] font-medium ${
                    pending
                      ? "bg-amber-500/10 text-amber-700"
                      : "bg-emerald-500/10 text-emerald-700"
                  }`}
                  data-testid={`api-fusion-target-status-${target.provider_id}`}
                >
                  {pending
                    ? t("apiFusionPendingSync", "Pending sync")
                    : t("apiFusionSynced", "Synced")}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
