import { ChevronDown, SlidersHorizontal } from "lucide-react";
import type { TFunction } from "i18next";
import {
  CANONICAL_REASONING_EFFORTS,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import {
  candidateNativeEfforts,
  isReservedNoneEffort,
  storedBudgetTokens,
  type DraftCandidate,
  type ReasoningDraft,
} from "./reasoningDraft";

type ReasoningEditorProps = {
  candidate: DraftCandidate;
  draft: ReasoningDraft;
  expanded: boolean;
  onToggleExpanded: () => void;
  onUpdate: (patch: Partial<ReasoningDraft>) => void;
  providers: GatewayUpstreamProvider[];
  busy: boolean;
  saving: boolean;
  t: TFunction;
};

export function ReasoningEditor({
  candidate,
  draft,
  expanded,
  onToggleExpanded,
  onUpdate,
  providers,
  busy,
  saving,
  t,
}: ReasoningEditorProps) {
  const base = candidate.reasoning ?? null;
  const nativeEfforts = candidateNativeEfforts(providers, candidate, draft);
  const nativePositiveEfforts = nativeEfforts.filter(
    (value) => !isReservedNoneEffort(value),
  );
  const storedProfile = base?.profile ?? "";
  const isManualProfile = storedProfile === "anthropic_manual_budget";
  const isSwitchProfile = storedProfile === "chat_thinking_switch";
  const budgetTokens = storedBudgetTokens(base);
  const noneOffersDisable =
    nativeEfforts.includes("none") || base?.supports_disable === true;
  const fieldClass =
    "h-8 rounded-lg border border-border/80 bg-background px-2 text-xs text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50";
  const isExpanded = expanded;

  const updateBinding = (effort: string, option: string) => {
    onUpdate({
      mappings: { ...draft.mappings, [effort]: option },
    });
  };

  const boundCount = CANONICAL_REASONING_EFFORTS.filter(
    (effort) => Boolean(draft.mappings[effort]),
  ).length;

  return (
    <div
      data-testid={`model-pool-reasoning-editor-${candidate.provider_id}-${candidate.local_model}`}
      className="w-full rounded-xl border border-border/60 bg-muted/20 overflow-hidden transition-all"
    >
      {/* 顶部标题栏兼折叠切换条 */}
      <button
        type="button"
        aria-expanded={isExpanded}
        onClick={onToggleExpanded}
        className="flex w-full items-center justify-between px-3 py-2 text-left hover:bg-muted/40 transition-colors"
      >
        <div className="flex items-center gap-2 min-w-0">
          <SlidersHorizontal className="h-3.5 w-3.5 text-primary/70 shrink-0" />
          <span className="text-xs font-medium text-foreground">
            {t(
              "aiGatewayModelPoolReasoningBindingsLabel",
              "Reasoning bindings",
            )}
          </span>
          {boundCount > 0 ? (
            <span className="rounded-full bg-primary/10 px-2 py-0.2 text-[10px] font-medium text-primary">
              {boundCount} 档已映射
            </span>
          ) : null}
        </div>
        <ChevronDown
          className={`h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform ${
            isExpanded ? "" : "-rotate-90"
          }`}
        />
      </button>

      {/* 配置表单主体 */}
      <div
        data-testid="model-pool-reasoning-body"
        className={
          isExpanded
            ? "space-y-3 border-t border-border/50 p-3 bg-background/40"
            : "hidden"
        }
      >
        {/* 主面板：把模型池统一档位绑定到该模型的原生档位 */}
        <div className="space-y-2">
          <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
            <div className="text-[11px] font-medium text-muted-foreground">
              {t(
                "aiGatewayModelPoolReasoningBindingsLabel",
                "Reasoning bindings",
              )}
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="text-[10px] text-muted-foreground">
                {t(
                  "aiGatewayModelPoolReasoningNativeEffortsLabel",
                  "Native levels",
                )}
                :
              </span>
              {nativeEfforts.length > 0 ? (
                nativeEfforts.map((value) => (
                  <span
                    key={value}
                    className="inline-flex items-center rounded border border-border/60 bg-muted/60 px-1.5 py-0.5 font-mono text-[10px] text-foreground/80"
                  >
                    {value}
                  </span>
                ))
              ) : (
                <span className="text-[10px] text-muted-foreground">
                  {t(
                    "aiGatewayModelPoolReasoningNoNativeEfforts",
                    "This model declares no native reasoning levels; configure reasoning levels in the provider model.",
                  )}
                </span>
              )}
            </div>
          </div>
          <p className="text-[10px] leading-relaxed text-muted-foreground">
            {t(
              "aiGatewayModelPoolReasoningBindingsHint",
              "Bind each pool level to a native level; only explicit bindings are saved.",
            )}
          </p>
          <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-2">
            {CANONICAL_REASONING_EFFORTS.map((effort) => {
              const isNone = effort === "none";
              const bindingOptions: Array<{ value: string; label: string }> =
                [];
              if (isNone) {
                if (noneOffersDisable) {
                  bindingOptions.push({
                    value: "disable",
                    label: t(
                      "aiGatewayModelPoolReasoningActionDisable",
                      "Disable",
                    ),
                  });
                }
              } else if (isManualProfile) {
                for (const tokens of budgetTokens) {
                  bindingOptions.push({
                    value: `budget:${tokens}`,
                    label: `${t(
                      "aiGatewayModelPoolReasoningActionBudget",
                      "Budget",
                    )} · ${tokens}`,
                  });
                }
              } else if (isSwitchProfile) {
                bindingOptions.push({
                  value: "enum:enabled",
                  label: "enabled",
                });
              } else {
                for (const value of nativePositiveEfforts) {
                  bindingOptions.push({
                    value: `enum:${value}`,
                    label: value,
                  });
                }
              }
              return (
                <div
                  key={effort}
                  className="flex flex-col gap-1.5 rounded-lg border border-border/50 bg-background/80 p-2 text-[11px] shadow-2xs"
                >
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-foreground/80">
                      {effort}
                    </span>
                    {draft.mappings[effort] ? (
                      <span className="h-1.5 w-1.5 rounded-full bg-primary" />
                    ) : null}
                  </div>
                  <select
                    data-testid={`model-pool-reasoning-mapping-${effort}`}
                    aria-label={`${t(
                      "aiGatewayModelPoolReasoningMappingLabel",
                      "Mapping matrix",
                    )}: ${effort}`}
                    value={draft.mappings[effort] ?? ""}
                    onChange={(event) =>
                      updateBinding(effort, event.target.value)
                    }
                    disabled={busy || saving}
                    className={fieldClass}
                  >
                    <option value="">
                      {t(
                        "aiGatewayModelPoolReasoningMappingUnmapped",
                        "Unmapped",
                      )}
                    </option>
                    {bindingOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </div>
              );
            })}
          </div>
        </div>

        {isSwitchProfile ? (
          <p className="rounded-md bg-amber-500/10 border border-amber-500/20 px-2.5 py-1.5 text-[11px] text-amber-700 dark:text-amber-400">
            {t(
              "aiGatewayModelPoolReasoningSwitchNoGradient",
              "This profile selects one intensity: every positive mapping sends the same switch, so there is no intensity gradient.",
            )}
          </p>
        ) : null}
      </div>
    </div>
  );
}
