import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Check,
  Copy,
  Layers,
  MoreVertical,
  Pencil,
  ScrollText,
  Trash2,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { useToast } from "@/components/ToastProvider";
import {
  MODEL_POOL_AVAILABILITY_LABEL_KEYS,
  expandModelPoolFallbackModel,
  resolveModelPoolCandidateState,
  selectPoolReasoningCoverageRow,
  type GatewayModelPool,
  type GatewayModelPoolFallbackModelStatus,
  type GatewayModelPoolFallbackSourceStatus,
  type GatewayModelPoolStatus,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import { protocolShortLabel } from "./gatewayShared";

type ModelPoolCardProps = {
  pool: GatewayModelPool;
  providers?: GatewayUpstreamProvider[];
  status?: GatewayModelPoolStatus | null;
  busy?: boolean;
  onToggleEnabled: (poolId: string, enabled: boolean) => void;
  onEdit: (pool: GatewayModelPool) => void;
  onDelete: (poolId: string) => void;
  onOpenLogs: (model: string) => void;
};

export function ModelPoolCard({
  pool,
  providers = [],
  status = null,
  busy = false,
  onToggleEnabled,
  onEdit,
  onDelete,
  onOpenLogs,
}: ModelPoolCardProps) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const [copied, setCopied] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const handleCopy = async () => {
    let ok = false;
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(pool.model);
        ok = true;
      }
    } catch {
      // Fall back to document.execCommand below
    }
    if (!ok) {
      try {
        const textarea = document.createElement("textarea");
        textarea.value = pool.model;
        textarea.style.position = "fixed";
        textarea.style.left = "-9999px";
        textarea.style.top = "-9999px";
        textarea.setAttribute("readonly", "true");
        document.body.appendChild(textarea);
        textarea.select();
        ok = document.execCommand("copy");
        document.body.removeChild(textarea);
      } catch {
        ok = false;
      }
    }
    if (ok) {
      setCopied(true);
      pushToast({
        title: t("aiGatewayModelListCopySuccess", "Model ID copied to clipboard"),
        kind: "success",
      });
      setTimeout(() => setCopied(false), 1500);
    } else {
      pushToast({
        title: t("aiGatewayCopyFailed", "Failed to copy to clipboard"),
        kind: "error",
      });
    }
  };

  const candidateCount = pool.candidates.length;
  const sourceCount = new Set(pool.candidates.map((c) => c.provider_id)).size;

  // Ordered fallback models are reported separately and never counted as
  // regular candidates or as part of regular readiness.
  const fallbackModels = (pool.fallback_models ?? [])
    .map((model) => model.trim())
    .filter((model) => model !== "");
  const fallbackStatusByModel = new Map<
    string,
    GatewayModelPoolFallbackModelStatus
  >(
    (status?.fallback_models ?? []).map(
      (item) => [item.model.trim(), item] as const,
    ),
  );

  // Pool capability is read only from this pool's own status snapshot; source
  // metadata is never promoted into a pool reasoning variant.
  const reasoningEfforts = status?.reasoning_efforts ?? [];
  const defaultEffortRaw = status?.default_reasoning_effort?.trim() ?? "";
  const coverage = status?.reasoning_coverage ?? [];
  // Surface the concrete default's row, else the first advertised labeled tier;
  // never silently fall back to the unlabeled canonical `low` row while another
  // tier is mapped.
  const coverageRow = selectPoolReasoningCoverageRow(
    coverage,
    defaultEffortRaw || "auto",
    reasoningEfforts,
  );
  const defaultLabel =
    defaultEffortRaw && defaultEffortRaw !== "auto"
      ? defaultEffortRaw
      : t("aiGatewayModelPoolReasoningDefaultAuto", "Provider default (auto)");

  return (
    <div
      data-testid={`ai-gateway-model-pool-${pool.id}`}
      className={`group relative flex flex-col justify-between rounded-xl border bg-card p-3.5 shadow-sm transition-all hover:border-primary/40 hover:shadow-md ${
        !pool.enabled ? "opacity-85 hover:opacity-100" : ""
      }`}
    >
      <div>
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-start gap-3 min-w-0 flex-1">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-2xs mt-0.5">
              <Layers className="h-4.5 w-4.5" />
            </div>
            <div className="min-w-0 flex-1">
              <button
                type="button"
                data-testid={`ai-gateway-model-pool-name-${pool.id}`}
                onClick={() => onEdit(pool)}
                className="cursor-pointer break-words text-left text-sm font-semibold text-foreground leading-5 transition hover:text-primary"
              >
                {pool.name}
              </button>
              <div className="mt-1 flex flex-wrap items-center gap-1.5">
                <code
                  data-testid={`ai-gateway-model-pool-model-${pool.id}`}
                  onClick={() => void handleCopy()}
                  title={t("aiGatewayModelPoolCopyAria", "Copy public model ID")}
                  className="inline-flex max-w-full cursor-pointer items-center break-all rounded-md border border-border/60 bg-muted/50 px-1.5 py-0.5 font-mono text-[11px] font-medium leading-4 text-foreground transition hover:bg-muted"
                >
                  {pool.model}
                </code>
                <span
                  className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[11px] font-medium leading-4 ${
                    pool.enabled
                      ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                      : "bg-muted text-muted-foreground"
                  }`}
                >
                  {pool.enabled
                    ? t("aiGatewayModelPoolIntentEnabled", "Enabled")
                    : t("aiGatewayModelPoolIntentDisabled", "Disabled")}
                </span>
              </div>
            </div>
          </div>

          <div className="flex shrink-0 items-center gap-1 pt-0.5">
            <Switch
              data-testid={`ai-gateway-model-pool-enable-${pool.id}`}
              aria-label={t("aiGatewayModelPoolEnableAria", {
                name: pool.name,
                defaultValue: "Enable model pool {{name}}",
              })}
              checked={pool.enabled}
              disabled={busy}
              onCheckedChange={(checked) => onToggleEnabled(pool.id, checked)}
            />
          </div>
        </div>

        {/* Counts + actions */}
        <div className="mt-2.5 flex items-center justify-between gap-2">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
            <span>
              {t("aiGatewayModelPoolCandidatesCount", {
                count: candidateCount,
                defaultValue: "{{count}} candidate(s)",
              })}
            </span>
            <span>
              {t("aiGatewayModelPoolSourcesCount", {
                count: sourceCount,
                defaultValue: "{{count}} source(s)",
              })}
            </span>
          </div>
          <div className="flex items-center gap-0.5">
            <button
              type="button"
              data-testid={`ai-gateway-model-pool-copy-${pool.id}`}
              onClick={() => void handleCopy()}
              aria-label={t("aiGatewayModelPoolCopyAria", "Copy public model ID")}
              title={t("aiGatewayModelPoolCopyAria", "Copy public model ID")}
              className="rounded p-1 text-muted-foreground transition hover:bg-muted hover:text-foreground"
            >
              {copied ? (
                <Check className="h-3.5 w-3.5 text-emerald-500" />
              ) : (
                <Copy className="h-3.5 w-3.5" />
              )}
            </button>
            <button
              type="button"
              data-testid={`ai-gateway-model-pool-edit-${pool.id}`}
              onClick={() => onEdit(pool)}
              aria-label={t("aiGatewayModelPoolEditAria", {
                name: pool.name,
                defaultValue: "Edit model pool {{name}}",
              })}
              title={t("aiGatewayModelPoolEditAria", {
                name: pool.name,
                defaultValue: "Edit model pool {{name}}",
              })}
              className="rounded p-1 text-muted-foreground transition hover:bg-muted hover:text-foreground"
            >
              <Pencil className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              data-testid={`ai-gateway-model-pool-log-${pool.id}`}
              onClick={() => onOpenLogs(pool.model)}
              aria-label={t("aiGatewayModelPoolLogAria", {
                model: pool.model,
                defaultValue: "View request logs for {{model}}",
              })}
              title={t("aiGatewayModelPoolLogAria", {
                model: pool.model,
                defaultValue: "View request logs for {{model}}",
              })}
              className="rounded p-1 text-muted-foreground transition hover:bg-muted hover:text-foreground"
            >
              <ScrollText className="h-3.5 w-3.5" />
            </button>
            <div className="relative">
              <button
                type="button"
                data-testid={`ai-gateway-model-pool-menu-${pool.id}`}
                aria-label={t("aiGatewayModelPoolMoreAria", "More actions")}
                aria-expanded={menuOpen}
                onClick={() => setMenuOpen((value) => !value)}
                className="rounded p-1 text-muted-foreground transition hover:bg-muted hover:text-foreground"
              >
                <MoreVertical className="h-3.5 w-3.5" />
              </button>
              {menuOpen ? (
                <div className="absolute right-0 z-30 mt-1 min-w-[140px] rounded-lg border border-border/80 bg-popover p-1 shadow-lg">
                  <button
                    type="button"
                    data-testid={`ai-gateway-model-pool-delete-${pool.id}`}
                    onClick={() => {
                      setMenuOpen(false);
                      setConfirmingDelete(true);
                    }}
                    className="flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-xs text-destructive transition hover:bg-destructive/10"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    <span>{t("aiGatewayModelPoolDelete", "Delete")}</span>
                  </button>
                </div>
              ) : null}
            </div>
          </div>
        </div>

        {fallbackModels.length > 0 ? (
          <div
            data-testid={`ai-gateway-model-pool-fallback-${pool.id}`}
            className="mt-2.5 space-y-1.5 rounded-lg border border-border/60 bg-muted/20 p-2"
          >
            <div className="text-[10px] font-medium text-muted-foreground">
              {t("aiGatewayModelPoolFallbackLabel", "Fallback models")}
            </div>
            <ul className="space-y-1.5">
              {fallbackModels.map((model) => {
                const record = fallbackStatusByModel.get(model) ?? null;
                const derived: GatewayModelPoolFallbackSourceStatus[] = record
                  ? []
                  : expandModelPoolFallbackModel(providers, model).map(
                      (candidate) => {
                        const state = resolveModelPoolCandidateState(
                          candidate,
                          providers,
                          null,
                        );
                        return {
                          provider_id: candidate.provider_id,
                          provider_name: state.provider_name,
                          local_model: candidate.local_model,
                          upstream_model: candidate.upstream_model,
                          protocol: candidate.protocol,
                          ready: state.ready,
                          availability: state.availability,
                          reason: state.reason,
                          cooling_until: state.cooling_until,
                        };
                      },
                    );
                const sources = record?.sources ?? derived;
                const readySources =
                  record?.ready_sources ??
                  sources.filter((source) => source.ready).length;
                const totalSources = record?.total_sources ?? sources.length;
                const ready = record?.ready ?? readySources > 0;
                const firstUnavailable = sources.find(
                  (source) => !source.ready,
                );
                const availability = ready
                  ? "ready"
                  : (firstUnavailable?.availability ??
                    (sources.length === 0 ? "mapping_unavailable" : "no_usable_key"));
                const labelKey =
                  MODEL_POOL_AVAILABILITY_LABEL_KEYS[availability];
                const label = labelKey ? t(labelKey) : availability;
                const reason =
                  record?.reason?.trim() ||
                  (!ready ? firstUnavailable?.reason?.trim() : null) ||
                  (sources.length === 0
                    ? t(
                        "aiGatewayModelPoolFallbackUnavailable",
                        "Unavailable — no provider mapping",
                      )
                    : null);
                return (
                  <li
                    key={model}
                    role="listitem"
                    className="space-y-1 rounded-md border border-border/50 bg-card/60 px-1.5 py-1"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-1.5">
                      <span className="break-all font-mono text-[11px] font-medium text-foreground">
                        {model}
                      </span>
                      <span
                        className={`inline-flex items-center rounded px-1.5 py-0.2 text-[10px] font-medium ${
                          ready
                            ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                            : "bg-amber-500/15 text-amber-700 dark:text-amber-400"
                        }`}
                      >
                        {label}
                      </span>
                    </div>
                    <div className="text-[10px] text-muted-foreground">
                      {t(
                        "aiGatewayModelPoolFallbackSourceCount",
                        "{{ready}} / {{total}} sources",
                        { ready: readySources, total: totalSources },
                      )}
                    </div>
                    {sources.map((source) => (
                      <div
                        key={`${source.provider_id}-${source.upstream_model}-${source.protocol}`}
                        className="flex flex-wrap items-center gap-1.5 text-[10px]"
                      >
                        <span className="font-medium text-foreground/85">
                          {source.provider_name ?? source.provider_id}
                        </span>
                        <code className="break-all rounded border border-border/40 bg-background px-1 py-0.2 font-mono text-foreground">
                          {source.upstream_model}
                        </code>
                        <span className="rounded bg-background px-1.5 py-0.2 text-muted-foreground">
                          {protocolShortLabel(source.protocol)}
                        </span>
                        <span className="text-muted-foreground">
                          {MODEL_POOL_AVAILABILITY_LABEL_KEYS[
                            source.availability
                          ]
                            ? t(
                                MODEL_POOL_AVAILABILITY_LABEL_KEYS[
                                  source.availability
                                ],
                              )
                            : source.availability}
                        </span>
                      </div>
                    ))}
                    {reason ? (
                      <div className="flex flex-wrap items-baseline gap-1 text-[10px] text-amber-700 dark:text-amber-400">
                        <span>
                          {t(
                            "aiGatewayModelPoolFallbackReason",
                            "Fallback reason",
                          )}
                          :
                        </span>
                        <span className="break-words">{reason}</span>
                      </div>
                    ) : null}
                  </li>
                );
              })}
            </ul>
          </div>
        ) : null}

        {/* Pool-configured reasoning capability; nothing is inferred from sources */}
        {reasoningEfforts.length > 0 ||
        defaultEffortRaw !== "" ||
        coverageRow ? (
          <div className="mt-2.5 space-y-1.5 rounded-lg border border-border/60 bg-muted/20 p-2">
            {reasoningEfforts.length > 0 ? (
              <div
                data-testid={`ai-gateway-model-pool-reasoning-support-${pool.id}`}
                className="flex flex-wrap items-center gap-1"
              >
                <span className="text-[10px] font-medium text-muted-foreground">
                  {t("aiGatewayReasoningEfforts", "Reasoning efforts")}:
                </span>
                {reasoningEfforts.map((effort) => (
                  <span
                    key={effort}
                    className="inline-flex items-center rounded border border-border/80 bg-background px-1.5 py-0.2 font-mono text-[10px] font-medium text-foreground/85 leading-tight"
                  >
                    {effort}
                  </span>
                ))}
              </div>
            ) : null}
            {defaultEffortRaw !== "" ? (
              <div className="flex items-center gap-1 text-[10px] text-muted-foreground">
                <span>{t("aiGatewayModelPoolReasoningDefaultLabel", "Reasoning default")}:</span>
                <span
                  data-testid={`ai-gateway-model-pool-reasoning-default-${pool.id}`}
                  className="rounded bg-background px-1.5 py-0.2 font-mono font-medium text-foreground/85"
                >
                  {defaultLabel}
                </span>
              </div>
            ) : null}
            {coverageRow ? (
              <div className="flex items-center gap-1 text-[10px] text-muted-foreground">
                <span>{t("aiGatewayModelPoolReasoningCoverageLabel", "Static coverage")}:</span>
                <span
                  data-testid={`ai-gateway-model-pool-reasoning-coverage-${pool.id}`}
                  className="font-medium text-foreground/85"
                >
                  <span className="font-mono">{coverageRow.effort}</span>{" "}
                  {t("aiGatewayModelPoolReasoningCoverageValue", {
                    mapped: coverageRow.mapped_candidates,
                    total: coverageRow.total_candidates,
                    defaultValue: "{{mapped}}/{{total}} candidates mapped",
                  })}
                </span>
              </div>
            ) : null}
          </div>
        ) : null}

        {/* Delete confirmation */}
        {confirmingDelete ? (
          <div className="mt-2 rounded-lg border border-destructive/30 bg-destructive/5 p-2.5">
            <div className="text-xs font-medium text-foreground">
              {t(
                "aiGatewayModelPoolDeleteConfirmBody",
                'Delete model pool "{{name}}"? This cannot be undone.',
                { name: pool.name },
              )}
            </div>
            <div className="mt-2 flex items-center justify-end gap-2">
              <button
                type="button"
                onClick={() => setConfirmingDelete(false)}
                disabled={busy}
                className="inline-flex h-7 items-center rounded-md border border-border/80 bg-background px-2.5 text-[11px] font-medium text-foreground transition hover:bg-muted disabled:opacity-50"
              >
                {t("aiGatewayModelPoolDeleteCancel", "Cancel")}
              </button>
              <button
                type="button"
                data-testid="ai-gateway-model-pool-delete-confirm"
                onClick={() => {
                  setConfirmingDelete(false);
                  onDelete(pool.id);
                }}
                disabled={busy}
                className="inline-flex h-7 items-center rounded-md bg-destructive px-2.5 text-[11px] font-medium text-destructive-foreground transition hover:bg-destructive/90 disabled:opacity-50"
              >
                {t("aiGatewayModelPoolDeleteConfirmButton", "Delete")}
              </button>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}
