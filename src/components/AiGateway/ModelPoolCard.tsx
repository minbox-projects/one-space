import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowRightLeft,
  Check,
  Copy,
  Layers,
  ScrollText,
  Server,
  Sparkles,
  Terminal,
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
  onOpenLogs: (model: string) => void;
};

export function ModelPoolCard({
  pool,
  providers = [],
  status = null,
  busy = false,
  onToggleEnabled,
  onEdit,
  onOpenLogs,
}: ModelPoolCardProps) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const [copied, setCopied] = useState(false);
  const copyTimerRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current);
      }
    };
  }, []);

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
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current);
      }
      copyTimerRef.current = window.setTimeout(() => setCopied(false), 1500);
    } else {
      pushToast({
        title: t("aiGatewayCopyFailed", "Failed to copy to clipboard"),
        kind: "error",
      });
    }
  };

  const candidateCount = pool.candidates.length;
  const sourceCount = new Set(pool.candidates.map((c) => c.provider_id)).size;

  const distinctProviders = useMemo(() => {
    const seen = new Set<string>();
    const result: Array<{ id: string; name: string }> = [];
    for (const c of pool.candidates) {
      if (!seen.has(c.provider_id)) {
        seen.add(c.provider_id);
        const found = providers.find((p) => p.id === c.provider_id);
        result.push({
          id: c.provider_id,
          name: found?.name?.trim() || c.provider_id,
        });
      }
    }
    return result;
  }, [pool.candidates, providers]);

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
            <div
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-2xs mt-0.5"
              aria-hidden="true"
            >
              <Layers className="h-4.5 w-4.5" />
            </div>
            <div className="min-w-0 flex-1">
              <button
                type="button"
                data-testid={`ai-gateway-model-pool-name-${pool.id}`}
                onClick={() => onEdit(pool)}
                className="truncate text-left text-sm font-semibold text-foreground hover:text-primary transition-colors block w-full leading-5"
                title={pool.name}
              >
                {pool.name}
              </button>
              <div className="mt-1 flex flex-wrap items-center gap-1.5">
                <span className="inline-flex items-center rounded-md bg-primary/10 px-1.5 py-0.5 text-[11px] font-medium leading-4 text-primary">
                  {t("aiGatewayModelPoolVirtualTag", "Model pool")}
                </span>
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

          <div className="shrink-0 pt-0.5">
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

        {/* 中间信息：公共模型 ID 与候选源及服务商统计 */}
        <div className="mt-2.5 space-y-1.5">
          <div className="flex items-center justify-between gap-2 rounded-lg border border-border/60 bg-muted/30 px-2.5 py-1 text-xs transition-colors hover:bg-muted/50">
            <div className="flex items-center gap-1.5 min-w-0 flex-1">
              <Terminal className="h-3.5 w-3.5 shrink-0 text-muted-foreground opacity-70" />
              <code
                data-testid={`ai-gateway-model-pool-model-${pool.id}`}
                onClick={() => void handleCopy()}
                title={t("aiGatewayModelPoolCopyAria", "Copy public model ID")}
                className="truncate cursor-pointer font-mono text-[11px] font-medium text-foreground hover:text-primary transition-colors block"
              >
                {pool.model}
              </code>
            </div>
            <div className="flex items-center gap-0.5 shrink-0">
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
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
            <ArrowRightLeft className="h-3.5 w-3.5 shrink-0 opacity-70" />
            <span>
              {t("aiGatewayModelPoolCandidatesCount", {
                count: candidateCount,
                defaultValue: "{{count}} candidate(s)",
              })}
            </span>
            <span className="opacity-30">·</span>
            <Server className="h-3.5 w-3.5 shrink-0 opacity-70" />
            <span>
              {t("aiGatewayModelPoolSourcesCount", {
                count: sourceCount,
                defaultValue: "{{count}} source(s)",
              })}
            </span>
          </div>

          {distinctProviders.length > 0 ? (
            <div
              data-testid={`ai-gateway-model-pool-providers-${pool.id}`}
              className="flex flex-wrap items-center gap-1 pt-0.5"
            >
              {distinctProviders.map((item) => (
                <span
                  key={item.id}
                  className="inline-flex items-center gap-1 rounded-md border border-border/70 bg-background px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground"
                  title={item.name}
                >
                  <span className="h-1.5 w-1.5 rounded-full bg-primary/60" />
                  <span className="truncate max-w-[120px]">@{item.name}</span>
                </span>
              ))}
            </div>
          ) : null}
        </div>

        {fallbackModels.length > 0 ? (
          <div
            data-testid={`ai-gateway-model-pool-fallback-${pool.id}`}
            className="mt-2.5 space-y-1.5 rounded-lg border border-border/60 bg-muted/20 p-2"
          >
            <div className="flex items-center justify-between text-[10px] font-medium text-muted-foreground">
              <span>{t("aiGatewayModelPoolFallbackLabel", "Fallback models")}</span>
              <span>
                {t("aiGatewayModelPoolFallbackCount", {
                  count: fallbackModels.length,
                  defaultValue: "{{count}} model(s)",
                })}
              </span>
            </div>
            <ul className="space-y-1">
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

                const sourceDetails = sources
                  .map(
                    (s) =>
                      `${s.provider_name ?? s.provider_id} (${s.upstream_model}, ${protocolShortLabel(s.protocol)})`,
                  )
                  .join("; ");
                const tooltipParts = [
                  `${model}: ${label}`,
                  t(
                    "aiGatewayModelPoolFallbackSourceCount",
                    "{{ready}} / {{total}} sources",
                    { ready: readySources, total: totalSources },
                  ),
                ];
                if (sourceDetails) {
                  tooltipParts.push(`[${sourceDetails}]`);
                }
                if (reason) {
                  tooltipParts.push(reason);
                }
                const tooltip = tooltipParts.join(" · ");

                return (
                  <li
                    key={model}
                    role="listitem"
                    title={tooltip}
                    className="flex items-center justify-between gap-1.5 rounded-md border border-border/50 bg-card/60 px-2 py-1 text-xs"
                  >
                    <span className="truncate font-mono text-[11px] font-medium text-foreground">
                      {model}
                    </span>
                    <div className="flex shrink-0 items-center gap-1.5">
                      <span className="text-[10px] text-muted-foreground">
                        {t(
                          "aiGatewayModelPoolFallbackSourceCount",
                          "{{ready}} / {{total}} sources",
                          { ready: readySources, total: totalSources },
                        )}
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
                  </li>
                );
              })}
            </ul>
          </div>
        ) : null}

        {/* Pool-configured reasoning capability; compact inline badges */}
        {reasoningEfforts.length > 0 ||
        defaultEffortRaw !== "" ||
        coverageRow ? (
          <div className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 rounded-lg border border-border/60 bg-muted/20 px-2.5 py-1 text-xs text-muted-foreground">
            <div className="flex items-center gap-1 shrink-0 font-medium text-foreground/80">
              <Sparkles className="h-3 w-3 text-primary/70 shrink-0" />
              <span className="text-[11px]">{t("aiGatewayReasoningEfforts", "Reasoning")}:</span>
            </div>

            {reasoningEfforts.length > 0 ? (
              <div
                data-testid={`ai-gateway-model-pool-reasoning-support-${pool.id}`}
                className="flex items-center gap-1"
              >
                {reasoningEfforts.map((effort) => (
                  <span
                    key={effort}
                    className="inline-flex items-center rounded border border-border/70 bg-background px-1.5 py-0.2 font-mono text-[10px] font-medium text-foreground/85 leading-tight"
                  >
                    {effort}
                  </span>
                ))}
              </div>
            ) : null}

            {defaultEffortRaw !== "" ? (
              <div className="flex items-center gap-1 text-[11px]">
                <span className="opacity-30">·</span>
                <span className="opacity-70">{t("aiGatewayModelPoolReasoningDefaultLabel", "Default")}:</span>
                <span
                  data-testid={`ai-gateway-model-pool-reasoning-default-${pool.id}`}
                  className="font-mono text-[10px] font-medium text-foreground/85"
                >
                  {defaultLabel}
                </span>
              </div>
            ) : null}

            {coverageRow ? (
              <div className="flex items-center gap-1 text-[11px]">
                <span className="opacity-30">·</span>
                <span
                  data-testid={`ai-gateway-model-pool-reasoning-coverage-${pool.id}`}
                  className="font-medium text-foreground/85 text-[10px]"
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
      </div>
    </div>
  );
}
