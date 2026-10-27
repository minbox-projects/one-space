import { Equal, Eraser, Layers, Server, Sparkles, Trash2 } from "lucide-react";
import type { TFunction } from "i18next";
import {
  formatContextWindowDisplay,
  MODEL_POOL_AVAILABILITY_LABEL_KEYS,
  resolveMappingContextWindow,
  resolveModelPoolCandidateState,
  type GatewayProviderTemplateView,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import { protocolBadgeClass, protocolShortLabel } from "../gatewayShared";
import { ReasoningEditor } from "./ReasoningEditor";
import {
  reasoningDraftFrom,
  type DraftCandidate,
  type ReasoningDraft,
} from "./reasoningDraft";
import type { ModelPoolCandidateStatus } from "./types";

type SelectedCandidatesSectionProps = {
  t: TFunction;
  selected: DraftCandidate[];
  providers: GatewayUpstreamProvider[];
  templates: GatewayProviderTemplateView[];
  statusByKey: Map<string, ModelPoolCandidateStatus>;
  weights: Record<string, string>;
  onWeightChange: (key: string, value: string) => void;
  getWeightPercentage: (key: string) => string;
  onRemoveCandidate: (key: string) => void;
  unavailableSelectedCount: number;
  onClearUnavailable: () => void;
  onEqualizeWeights: () => void;
  busy: boolean;
  saving: boolean;
  reasoningDrafts: Record<string, ReasoningDraft>;
  expandedReasoning: Record<string, boolean>;
  onToggleReasoningExpanded: (key: string) => void;
  onUpdateReasoning: (
    candidate: DraftCandidate,
    patch: Partial<ReasoningDraft>,
  ) => void;
};

export function SelectedCandidatesSection({
  t,
  selected,
  providers,
  templates,
  statusByKey,
  weights,
  onWeightChange,
  getWeightPercentage,
  onRemoveCandidate,
  unavailableSelectedCount,
  onClearUnavailable,
  onEqualizeWeights,
  busy,
  saving,
  reasoningDrafts,
  expandedReasoning,
  onToggleReasoningExpanded,
  onUpdateReasoning,
}: SelectedCandidatesSectionProps) {
  return (
    <div className="lg:col-span-7 flex flex-col rounded-2xl border border-border/70 bg-card/40 overflow-hidden shadow-2xs">
      {/* 头部：标题与一键平分权重 */}
      <div className="p-3 border-b border-border/60 bg-muted/20 flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-foreground">
            {t("aiGatewayModelPoolSelectedTitle", "Selected candidates")}
          </span>
          <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
            {selected.length}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {unavailableSelectedCount > 0 ? (
            <button
              type="button"
              data-testid="model-pool-clear-unavailable"
              onClick={onClearUnavailable}
              disabled={busy || saving}
              className="inline-flex items-center gap-1 rounded-md border border-border/60 bg-background/80 px-2 py-0.5 text-[11px] font-medium text-muted-foreground hover:text-foreground hover:bg-muted transition-colors disabled:opacity-50"
            >
              <Eraser className="h-3 w-3 text-primary/80" />
              <span>
                {t(
                  "aiGatewayModelPoolClearUnavailable",
                  "Clear unavailable ({{count}})",
                  { count: unavailableSelectedCount },
                )}
              </span>
            </button>
          ) : null}
          {selected.length > 1 ? (
            <button
              type="button"
              onClick={onEqualizeWeights}
              disabled={busy || saving}
              title="将所有候选模型权重均等平分"
              className="inline-flex items-center gap-1 rounded-md border border-border/60 bg-background/80 px-2 py-0.5 text-[11px] font-medium text-muted-foreground hover:text-foreground hover:bg-muted transition-colors disabled:opacity-50"
            >
              <Equal className="h-3 w-3 text-primary/80" />
              <span>平分权重</span>
            </button>
          ) : null}
        </div>
      </div>

      {/* 已选列表 */}
      <div
        data-testid="model-pool-selected-list"
        className="max-h-[460px] overflow-y-auto p-2.5 sm:p-3 space-y-2.5"
      >
        {selected.length === 0 ? (
          <div className="rounded-xl border border-dashed border-border/70 bg-card/40 px-3 py-10 text-center flex flex-col items-center justify-center gap-1.5 text-muted-foreground">
            <Layers className="h-6 w-6 opacity-40 mb-1" />
            <p className="text-xs font-medium">
              {t(
                "aiGatewayModelPoolSelectedEmpty",
                "Select at least one source model.",
              )}
            </p>
            <p className="text-[11px] opacity-70">
              从左侧列表中勾选模型，将其加入当前模型池并分配路由权重
            </p>
          </div>
        ) : (
          selected.map((candidate) => {
            const state = resolveModelPoolCandidateState(
              candidate,
              providers,
              statusByKey.get(candidate.key) ?? null,
            );
            const labelKey =
              MODEL_POOL_AVAILABILITY_LABEL_KEYS[state.availability];
            const label = labelKey ? t(labelKey) : state.availability;
            const providerName =
              state.provider_name ?? candidate.provider_id;
            const candidateProvider = providers.find(
              (p) => p.id === candidate.provider_id,
            );
            const candidateTemplate = candidateProvider?.template_id
              ? templates.find(
                  (t) => t.template.id === candidateProvider.template_id,
                )?.template
              : undefined;
            const candidateTemplateName = candidateTemplate?.name ?? null;
            const weightVal = weights[candidate.key] ?? "1";
            const weightPercent = getWeightPercentage(candidate.key);
            const draft =
              reasoningDrafts[candidate.key] ??
              reasoningDraftFrom(candidate.reasoning);

            return (
              <div
                key={candidate.key}
                className="flex flex-col rounded-xl border border-border/70 bg-background/80 p-3 shadow-2xs hover:border-primary/40 transition-colors space-y-2.5"
              >
                {/* 顶部主行：模型名与右侧操作 */}
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0 flex-1 flex flex-wrap items-center gap-1.5">
                    <span
                      className="font-semibold text-xs sm:text-sm text-foreground break-all"
                      title={candidate.local_model}
                    >
                      {candidate.local_model}
                    </span>
                    <span className="text-muted-foreground/60 text-xs shrink-0 select-none">
                      →
                    </span>
                    <code
                      className="rounded-md border border-border/60 bg-muted/80 px-1.5 py-0.5 font-mono text-xs text-foreground break-all shadow-2xs"
                      title={candidate.upstream_model}
                    >
                      {candidate.upstream_model}
                    </code>
                  </div>

                  {/* 权重输入与删除 */}
                  <div className="flex items-center gap-1.5 shrink-0">
                    <div className="flex items-center gap-1.5 bg-muted/40 border border-border/60 rounded-lg px-2 py-0.5 shadow-2xs">
                      <span className="text-[10px] font-medium text-muted-foreground select-none">
                        {t("aiGatewayModelPoolWeightLabel", "weight")}
                      </span>
                      <input
                        type="number"
                        min={1}
                        max={100}
                        step={1}
                        value={weightVal}
                        onChange={(event) =>
                          onWeightChange(candidate.key, event.target.value)
                        }
                        aria-label={t("aiGatewayModelPoolWeightAria", {
                          model: candidate.local_model,
                          defaultValue: "{{model}} weight",
                        })}
                        disabled={busy || saving}
                        className="h-6 w-11 rounded border border-border/70 bg-background px-1 text-center font-mono text-xs text-foreground shadow-2xs outline-none focus:border-primary/60 focus-visible:ring-1 focus-visible:ring-primary/20 disabled:opacity-50"
                      />
                      <span className="text-[10px] font-mono text-primary font-semibold select-none">
                        {weightPercent}
                      </span>
                    </div>
                    <button
                      type="button"
                      onClick={() => onRemoveCandidate(candidate.key)}
                      disabled={busy || saving}
                      aria-label={t("aiGatewayModelPoolRemoveAria", {
                        model: candidate.local_model,
                        defaultValue: "Remove {{model}}",
                      })}
                      className="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition disabled:opacity-50"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </div>

                {/* 次行：属性与状态 */}
                <div className="flex flex-wrap items-center gap-1.5 text-xs">
                  <span className="inline-flex items-center gap-1 rounded-md border border-border/50 bg-muted/60 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                    <Server className="h-2.5 w-2.5 text-muted-foreground/80 shrink-0" />
                    <span
                      className="font-medium text-foreground/90 truncate max-w-[120px]"
                      title={providerName}
                    >
                      {providerName}
                    </span>
                    {candidateTemplateName ? (
                      <span
                        data-testid={`model-pool-selected-template-badge-${candidate.provider_id}`}
                        className="inline-flex items-center gap-0.5 rounded border border-indigo-500/25 bg-indigo-500/10 px-1 py-0.2 text-[9px] font-medium text-indigo-700 dark:text-indigo-300 shrink-0"
                        title={candidateTemplateName}
                      >
                        <Sparkles className="h-2 w-2" />
                        <span>{candidateTemplateName}</span>
                      </span>
                    ) : null}
                  </span>

                  <span
                    className={`inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium ${protocolBadgeClass(
                      candidate.protocol,
                    )}`}
                  >
                    {protocolShortLabel(candidate.protocol)}
                  </span>

                  {(() => {
                    const contextWindow = resolveMappingContextWindow(
                      providers,
                      candidate.provider_id,
                      candidate.local_model,
                      candidate.upstream_model,
                    );
                    if (!contextWindow) return null;
                    return (
                      <span
                        data-testid={`model-pool-selected-context-${candidate.key}`}
                        className="inline-flex items-center gap-1 rounded-md border border-cyan-500/25 bg-cyan-500/10 px-1.5 py-0.5 font-mono text-[10px] font-medium text-cyan-700 dark:text-cyan-300 shadow-2xs"
                        title={t(
                          "aiGatewayModelListEffectiveContext",
                          "Conservative effective context",
                        )}
                      >
                        <Layers className="h-2.5 w-2.5 text-cyan-600/80 dark:text-cyan-400/80 shrink-0" />
                        <span>{formatContextWindowDisplay(contextWindow)}</span>
                      </span>
                    );
                  })()}

                  <span
                    className={`inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium ${
                      state.ready
                        ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20"
                        : "bg-amber-500/15 text-amber-700 dark:text-amber-400 border border-amber-500/25"
                    }`}
                  >
                    {label}
                  </span>

                  {state.reason ? (
                    <span
                      className="inline-flex items-center rounded border border-amber-500/20 bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-700 dark:text-amber-400 break-words"
                      title={state.reason}
                    >
                      {state.reason}
                    </span>
                  ) : null}
                </div>

                {/* 内嵌推理策略绑定面板 */}
                <ReasoningEditor
                  candidate={candidate}
                  draft={draft}
                  expanded={Boolean(expandedReasoning[candidate.key])}
                  onToggleExpanded={() =>
                    onToggleReasoningExpanded(candidate.key)
                  }
                  onUpdate={(patch) => onUpdateReasoning(candidate, patch)}
                  providers={providers}
                  busy={busy}
                  saving={saving}
                  t={t}
                />
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
