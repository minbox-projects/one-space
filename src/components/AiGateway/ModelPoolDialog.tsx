import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  CheckSquare,
  ChevronDown,
  Layers,
  Search,
  Server,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { errorToMessage } from "@/lib/messages";
import {
  CANONICAL_REASONING_EFFORTS,
  MODEL_POOL_AVAILABILITY_LABEL_KEYS,
  MODEL_POOL_MIN_REASONING_BUDGET,
  modelPoolCandidateKey,
  modelPoolCandidateServesEffort,
  newModelPoolId,
  resolveModelPoolCandidateState,
  resolveModelPoolDefaultEffort,
  selectPoolReasoningCoverageRow,
  type GatewayModelPool,
  type GatewayModelPoolCandidate,
  type GatewayModelPoolReasoning,
  type GatewayModelPoolStatus,
  type GatewayProviderTemplateView,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
  type ModelPoolReasoningAction,
} from "@/lib/aiGateway";
import { protocolBadgeClass, protocolShortLabel } from "./gatewayShared";

type DraftCandidate = GatewayModelPoolCandidate & { key: string };

/** The five bounded native reasoning profiles the editor can declare. */
const REASONING_PROFILE_VALUES = [
  "openai_effort",
  "anthropic_effort_inherit",
  "anthropic_effort_adaptive",
  "anthropic_manual_budget",
  "chat_thinking_switch",
] as const;

/** Localized visible labels for the bounded profiles; values stay the tokens. */
const REASONING_PROFILE_LABEL_KEYS: Record<string, string> = {
  openai_effort: "aiGatewayModelPoolReasoningProfileOpenaiEffort",
  anthropic_effort_inherit:
    "aiGatewayModelPoolReasoningProfileAnthropicEffortInherit",
  anthropic_effort_adaptive:
    "aiGatewayModelPoolReasoningProfileAnthropicEffortAdaptive",
  anthropic_manual_budget:
    "aiGatewayModelPoolReasoningProfileAnthropicManualBudget",
  chat_thinking_switch:
    "aiGatewayModelPoolReasoningProfileChatThinkingSwitch",
};

/** Editable candidate-local reasoning draft; every field is a string in the UI. */
type ReasoningDraft = {
  profile: string;
  values: string;
  budgetMin: string;
  budgetMax: string;
  supportsDisable: boolean;
  /** public effort -> mapping option (`""` unmapped, `enum:<v>`, `budget`, `disable`). */
  mappings: Record<string, string>;
  /** public effort -> the mapping's own native budget tokens (as typed). */
  budgetTokens: Record<string, string>;
};

function actionToOptionValue(action: ModelPoolReasoningAction): string {
  if (action?.kind === "enum" && typeof action.value === "string") {
    return `enum:${action.value}`;
  }
  if (action?.kind === "budget") return "budget";
  if (action?.kind === "disable") return "disable";
  return "";
}

function reasoningDraftFrom(
  reasoning: GatewayModelPoolReasoning | null | undefined,
): ReasoningDraft {
  const mappings: Record<string, string> = {};
  const budgetTokens: Record<string, string> = {};
  for (const mapping of reasoning?.mappings ?? []) {
    const option = actionToOptionValue(mapping.action);
    if (option !== "") mappings[mapping.effort] = option;
    if (mapping.action?.kind === "budget") {
      const tokens = mapping.action.budget_tokens;
      budgetTokens[mapping.effort] =
        typeof tokens === "number" ? String(tokens) : "";
    }
  }
  return {
    profile: reasoning?.profile ?? "",
    values: (reasoning?.values ?? []).join(", "),
    budgetMin:
      reasoning?.budget?.min != null ? String(reasoning.budget.min) : "",
    budgetMax:
      reasoning?.budget?.max != null ? String(reasoning.budget.max) : "",
    supportsDisable: reasoning?.supports_disable === true,
    mappings,
    budgetTokens,
  };
}

function splitDeclaredValues(values: string): string[] {
  const result: string[] = [];
  for (const part of values.split(",")) {
    const trimmed = part.trim();
    if (trimmed !== "" && !result.includes(trimmed)) result.push(trimmed);
  }
  return result;
}

function parseOptionalInt(value: string): number | null {
  const trimmed = value.trim();
  if (trimmed === "") return null;
  const parsed = Number(trimmed);
  return Number.isInteger(parsed) ? parsed : null;
}

/** Convert one mapping option back into its typed native action. */
function optionValueToAction(
  option: string,
  draft: ReasoningDraft,
  effort: string,
): ModelPoolReasoningAction | null {
  if (option.startsWith("enum:")) {
    return { kind: "enum", value: option.slice("enum:".length) };
  }
  if (option === "budget") {
    // Each manual-budget mapping owns its own native token count; the declared
    // range only constrains it. The count is never substituted with the range.
    const tokens = parseOptionalInt(draft.budgetTokens[effort] ?? "");
    return tokens != null
      ? { kind: "budget", budget_tokens: tokens }
      : { kind: "budget" };
  }
  if (option === "disable") return { kind: "disable" };
  return null;
}

/** Build a candidate's persisted reasoning payload from its draft. */
function reasoningDraftPayload(draft: ReasoningDraft): GatewayModelPoolReasoning {
  const values = splitDeclaredValues(draft.values);
  const min = parseOptionalInt(draft.budgetMin);
  const max = parseOptionalInt(draft.budgetMax);
  const mappings = CANONICAL_REASONING_EFFORTS.flatMap((effort) => {
    const option = draft.mappings[effort] ?? "";
    const action = optionValueToAction(option, draft, effort);
    return action ? [{ effort, action }] : [];
  });
  return {
    profile: draft.profile,
    values,
    ...(min != null || max != null
      ? { budget: { min: min ?? undefined, max: max ?? undefined } }
      : {}),
    supports_disable: draft.supportsDisable,
    mappings,
  };
}

/** Whether a draft declares anything at all; an untouched candidate stays unknown. */
function hasDeclaredReasoning(draft: ReasoningDraft): boolean {
  return (
    draft.profile !== "" ||
    splitDeclaredValues(draft.values).length > 0 ||
    draft.supportsDisable ||
    Object.values(draft.mappings).some((option) => option !== "")
  );
}

/** The declared manual-budget range of one draft, distinguishing absent from invalid. */
function declaredBudgetRange(draft: ReasoningDraft): {
  min: number | null;
  max: number | null;
  declared: boolean;
  invalid: boolean;
} {
  const minRaw = draft.budgetMin.trim();
  const maxRaw = draft.budgetMax.trim();
  const declared = minRaw !== "" || maxRaw !== "";
  const min = minRaw === "" ? null : parseOptionalInt(minRaw);
  const max = maxRaw === "" ? null : parseOptionalInt(maxRaw);
  return {
    min,
    max,
    declared,
    invalid: declared && (min === null || max === null),
  };
}

/**
 * The localizable key of the first declaration the backend would refuse, or
 * `null`. It rejects a fractional/absent/out-of-range budget token, a declared
 * range below the adapter minimum and an inverted range before any write, so the
 * operator keeps the whole draft and sees a localized reason.
 */
function draftReasoningError(
  drafts: ReasoningDraft[],
): "aiGatewayModelPoolErrorReasoningDefaultUnmapped" | null {
  const key = "aiGatewayModelPoolErrorReasoningDefaultUnmapped" as const;
  for (const draft of drafts) {
    if (!hasDeclaredReasoning(draft)) continue;
    const range = declaredBudgetRange(draft);
    if (range.declared) {
      if (range.invalid) return key;
      if ((range.min as number) < MODEL_POOL_MIN_REASONING_BUDGET) return key;
      if ((range.max as number) < (range.min as number)) return key;
    }
    const budgetEfforts = CANONICAL_REASONING_EFFORTS.filter(
      (effort) => draft.mappings[effort] === "budget",
    );
    if (budgetEfforts.length > 0) {
      if (!range.declared || range.invalid) return key;
      for (const effort of budgetEfforts) {
        const tokens = parseOptionalInt(draft.budgetTokens[effort] ?? "");
        if (tokens === null) return key;
        if (tokens < (range.min as number) || tokens > (range.max as number)) {
          return key;
        }
      }
    }
  }
  return null;
}

/** Stable full-identity declaration signature for one effort, order-independent. */
function effortDeclarationSignature(
  rows: Array<{ key: string; action: ModelPoolReasoningAction | null }>,
): string {
  const entries = rows
    .filter(
      (row): row is { key: string; action: ModelPoolReasoningAction } =>
        row.action != null,
    )
    .map((row) => [row.key, JSON.stringify(row.action)] as const)
    .sort((a, b) => a[0].localeCompare(b[0]));
  return JSON.stringify(entries);
}

type ModelPoolDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  providers: GatewayUpstreamProvider[];
  /** Loaded template views, used to resolve a provider's bound template name. */
  templates?: GatewayProviderTemplateView[];
  /** The saved pool being edited; `null`/absent creates a new pool. */
  pool?: GatewayModelPool | null;
  /** Latest nonsecret status for the edited pool, used only for reasons. */
  poolStatus?: GatewayModelPoolStatus | null;
  busy?: boolean;
  /** Persist the pool; a rejected promise keeps the draft and surfaces its message. */
  onSave: (pool: GatewayModelPool) => Promise<void>;
};

function toDraft(candidate: GatewayModelPoolCandidate): DraftCandidate {
  return { ...candidate, key: modelPoolCandidateKey(candidate) };
}

function parseWeight(value: string): number | null {
  const trimmed = value.trim();
  if (trimmed === "") return null;
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed < 1 || parsed > 100) return null;
  return parsed;
}

export function ModelPoolDialog({
  open,
  onOpenChange,
  providers,
  templates = [],
  pool = null,
  poolStatus = null,
  busy = false,
  onSave,
}: ModelPoolDialogProps) {
  const { t } = useTranslation();

  // Seeded once per mount; the console remounts this dialog per open/target so
  // an in-flight runtime refresh can never overwrite an unsaved draft.
  const [name, setName] = useState(pool ? pool.name : "Free Modle");
  const [model, setModel] = useState(pool ? pool.model : "free-model");
  const [selected, setSelected] = useState<DraftCandidate[]>(() =>
    (pool?.candidates ?? []).map(toDraft),
  );
  const [weights, setWeights] = useState<Record<string, string>>(() => {
    const initial: Record<string, string> = {};
    for (const candidate of pool?.candidates ?? []) {
      initial[modelPoolCandidateKey(candidate)] = String(candidate.weight);
    }
    return initial;
  });
  // Pool default preference; a new pool starts at `medium`, a legacy pool at `auto`.
  const [defaultEffort, setDefaultEffort] = useState<string>(() =>
    pool ? resolveModelPoolDefaultEffort(pool) : "medium",
  );
  const [reasoningDrafts, setReasoningDrafts] = useState<
    Record<string, ReasoningDraft>
  >(() => {
    const initial: Record<string, ReasoningDraft> = {};
    for (const candidate of pool?.candidates ?? []) {
      initial[modelPoolCandidateKey(candidate)] = reasoningDraftFrom(
        candidate.reasoning,
      );
    }
    return initial;
  });
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  // 默认仅展开第一个已启用的服务商，后续已启用的服务商默认折叠
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>(() => {
    const initial: Record<string, boolean> = {};
    const enabledList = providers.filter((p) => p.enabled !== false);
    enabledList.forEach((provider, index) => {
      if (index > 0) {
        initial[provider.id] = true;
      }
    });
    return initial;
  });
  const [saving, setSaving] = useState(false);

  // 来源模型中仅显示已启用的服务商
  const activeProviders = useMemo(
    () => providers.filter((p) => p.enabled !== false),
    [providers],
  );

  const statusByKey = useMemo(() => {
    const map = new Map<string, GatewayModelPoolStatus["candidates"][number]>();
    for (const candidate of poolStatus?.candidates ?? []) {
      map.set(modelPoolCandidateKey(candidate), candidate);
    }
    return map;
  }, [poolStatus]);

  const sourceGroups = useMemo(() => {
    const groups = activeProviders.map((provider) => {
      const template = provider.template_id
        ? templates.find((t) => t.template.id === provider.template_id)?.template
        : undefined;
      return {
        providerId: provider.id,
        providerName: provider.name,
        templateName: template?.name ?? null,
        rows: (provider.mappings ?? [])
          .map((mapping) => {
            const localModel = mapping.local_model.trim();
            const upstreamModel = mapping.upstream_model.trim();
            if (!localModel || !upstreamModel) return null;
            const candidate: GatewayModelPoolCandidate = {
              provider_id: provider.id,
              local_model: localModel,
              upstream_model: upstreamModel,
              protocol:
                (mapping.protocol as GatewayUpstreamProtocol | undefined) ??
                provider.protocol ??
                "chat_completions",
              weight: 1,
            };
            return { key: modelPoolCandidateKey(candidate), candidate };
          })
          .filter((row): row is { key: string; candidate: GatewayModelPoolCandidate } =>
            row !== null,
          ),
      };
    });
    return groups;
  }, [activeProviders, templates]);

  const normalizedQuery = query.trim().toLowerCase();

  const selectedKeys = useMemo(
    () => new Set(selected.map((candidate) => candidate.key)),
    [selected],
  );

  // 当前搜索过滤条件下的所有可见来源候选模型
  const filteredCandidates = useMemo(() => {
    const list: GatewayModelPoolCandidate[] = [];
    for (const group of sourceGroups) {
      const visibleRows = group.rows.filter((row) =>
        normalizedQuery
          ? row.candidate.local_model.toLowerCase().includes(normalizedQuery) ||
            row.candidate.upstream_model.toLowerCase().includes(normalizedQuery) ||
            group.providerName.toLowerCase().includes(normalizedQuery)
          : true,
      );
      for (const r of visibleRows) {
        list.push(r.candidate);
      }
    }
    return list;
  }, [sourceGroups, normalizedQuery]);

  const allFilteredSelected = useMemo(() => {
    if (filteredCandidates.length === 0) return false;
    return filteredCandidates.every((candidate) =>
      selectedKeys.has(modelPoolCandidateKey(candidate)),
    );
  }, [filteredCandidates, selectedKeys]);

  // Live static coverage derived from the current draft declarations and the
  // current provider list; readiness stays a separate, key/health-aware count.
  const draftReasoningCatalog = useMemo(() => {
    const candidates = selected.map((candidate) => {
      const draft =
        reasoningDrafts[candidate.key] ??
        reasoningDraftFrom(candidate.reasoning);
      const reasoning = hasDeclaredReasoning(draft)
        ? reasoningDraftPayload(draft)
        : undefined;
      return { ...candidate, reasoning };
    });
    const coverage = CANONICAL_REASONING_EFFORTS.map((effort) => ({
      effort,
      mapped_candidates: candidates.filter((candidate) =>
        modelPoolCandidateServesEffort(providers, candidate, effort),
      ).length,
      total_candidates: candidates.length,
    }));
    return {
      coverage,
      supported: coverage
        .filter((row) => row.mapped_candidates > 0)
        .map((row) => row.effort),
    };
  }, [providers, reasoningDrafts, selected]);

  const coverageRow = selectPoolReasoningCoverageRow(
    draftReasoningCatalog.coverage,
    defaultEffort,
    draftReasoningCatalog.supported,
  );
  const readyCount = selected.filter(
    (candidate) =>
      resolveModelPoolCandidateState(
        candidate,
        providers,
        statusByKey.get(candidate.key) ?? null,
      ).ready,
  ).length;

  const ensureReasoningDrafts = (candidates: GatewayModelPoolCandidate[]) => {
    setReasoningDrafts((previous) => {
      let next = previous;
      for (const candidate of candidates) {
        const key = modelPoolCandidateKey(candidate);
        if (next[key] === undefined) {
          if (next === previous) next = { ...previous };
          next[key] = reasoningDraftFrom(candidate.reasoning);
        }
      }
      return next;
    });
  };

  const updateReasoning = (
    candidate: DraftCandidate,
    patch: Partial<ReasoningDraft>,
  ) => {
    setReasoningDrafts((previous) => ({
      ...previous,
      [candidate.key]: {
        ...(previous[candidate.key] ?? reasoningDraftFrom(candidate.reasoning)),
        ...patch,
      },
    }));
    setError(null);
  };

  const toggleSelectAllFiltered = () => {
    if (filteredCandidates.length === 0) return;
    if (allFilteredSelected) {
      // 取消全选：移除所有属于当前筛选结果的候选项
      const keysToRemove = new Set(
        filteredCandidates.map((c) => modelPoolCandidateKey(c)),
      );
      setSelected((prev) => prev.filter((item) => !keysToRemove.has(item.key)));
    } else {
      // 全选：将尚未在已选中的筛选模型全部加入
      const newItems: DraftCandidate[] = [];
      const newWeights: Record<string, string> = {};
      for (const c of filteredCandidates) {
        const key = modelPoolCandidateKey(c);
        if (!selectedKeys.has(key)) {
          newItems.push({ ...c, weight: 1, key });
          if (weights[key] === undefined) {
            newWeights[key] = "1";
          }
        }
      }
      setSelected((prev) => [...prev, ...newItems]);
      if (Object.keys(newWeights).length > 0) {
        setWeights((prev) => ({ ...prev, ...newWeights }));
      }
      ensureReasoningDrafts(newItems);
    }
    setError(null);
  };

  const toggleCandidate = (candidate: GatewayModelPoolCandidate) => {
    const key = modelPoolCandidateKey(candidate);
    setSelected((previous) => {
      if (previous.some((item) => item.key === key)) {
        return previous.filter((item) => item.key !== key);
      }
      return [...previous, { ...candidate, weight: 1, key }];
    });
    setWeights((previous) => {
      if (previous[key] !== undefined) return previous;
      return { ...previous, [key]: "1" };
    });
    ensureReasoningDrafts([candidate]);
    setError(null);
  };

  const removeCandidate = (key: string) => {
    setSelected((previous) => previous.filter((item) => item.key !== key));
    setError(null);
  };

  const reasoningEditor = (candidate: DraftCandidate) => {
    const draft =
      reasoningDrafts[candidate.key] ??
      reasoningDraftFrom(candidate.reasoning);
    const declaredValues = splitDeclaredValues(draft.values);
    const enumOptions = Array.from(
      new Set([
        ...declaredValues,
        ...Object.values(draft.mappings)
          .filter((option) => option.startsWith("enum:"))
          .map((option) => option.slice("enum:".length)),
      ]),
    );
    const fieldClass =
      "h-8 rounded-lg border border-border/80 bg-background px-2 text-xs text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50";
    return (
      <div
        data-testid={`model-pool-reasoning-editor-${candidate.provider_id}-${candidate.local_model}`}
        className="mt-0.5 space-y-2.5 border-t border-border/50 pt-2.5"
      >
        <div className="grid gap-2 sm:grid-cols-2">
          <label className="flex flex-col gap-1 text-[10px] font-medium text-muted-foreground">
            <span>
              {t(
                "aiGatewayModelPoolReasoningProfileLabel",
                "Native reasoning profile",
              )}
            </span>
            <select
              aria-label={t(
                "aiGatewayModelPoolReasoningProfileLabel",
                "Native reasoning profile",
              )}
              value={draft.profile}
              onChange={(event) =>
                updateReasoning(candidate, { profile: event.target.value })
              }
              disabled={busy || saving}
              className={fieldClass}
            >
              <option value="">
                {t("aiGatewayModelPoolReasoningProfileUnknown", "Unknown")}
              </option>
              {REASONING_PROFILE_VALUES.map((profile) => (
                <option key={profile} value={profile}>
                  {t(REASONING_PROFILE_LABEL_KEYS[profile] ?? profile, profile)}
                </option>
              ))}
              {draft.profile !== "" &&
              !(REASONING_PROFILE_VALUES as readonly string[]).includes(
                draft.profile,
              ) ? (
                <option value={draft.profile}>{draft.profile}</option>
              ) : null}
            </select>
          </label>
          <label className="flex flex-col gap-1 text-[10px] font-medium text-muted-foreground">
            <span>
              {t(
                "aiGatewayModelPoolReasoningValuesLabel",
                "Declared values",
              )}
            </span>
            <input
              type="text"
              aria-label={t(
                "aiGatewayModelPoolReasoningValuesLabel",
                "Declared values",
              )}
              value={draft.values}
              onChange={(event) =>
                updateReasoning(candidate, { values: event.target.value })
              }
              disabled={busy || saving}
              className={fieldClass}
            />
          </label>
        </div>
        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1 text-[10px] font-medium text-muted-foreground">
            <span>
              {t(
                "aiGatewayModelPoolReasoningBudgetMinLabel",
                "Minimum budget",
              )}
            </span>
            <input
              type="number"
              min={0}
              step={1}
              aria-label={t(
                "aiGatewayModelPoolReasoningBudgetMinLabel",
                "Minimum budget",
              )}
              value={draft.budgetMin}
              onChange={(event) =>
                updateReasoning(candidate, { budgetMin: event.target.value })
              }
              disabled={busy || saving}
              className={`${fieldClass} w-28`}
            />
          </label>
          <label className="flex flex-col gap-1 text-[10px] font-medium text-muted-foreground">
            <span>
              {t(
                "aiGatewayModelPoolReasoningBudgetMaxLabel",
                "Maximum budget",
              )}
            </span>
            <input
              type="number"
              min={0}
              step={1}
              aria-label={t(
                "aiGatewayModelPoolReasoningBudgetMaxLabel",
                "Maximum budget",
              )}
              value={draft.budgetMax}
              onChange={(event) =>
                updateReasoning(candidate, { budgetMax: event.target.value })
              }
              disabled={busy || saving}
              className={`${fieldClass} w-28`}
            />
          </label>
          <label className="inline-flex items-center gap-1.5 pb-1.5 text-[10px] font-medium text-muted-foreground">
            <input
              type="checkbox"
              aria-label={t(
                "aiGatewayModelPoolReasoningSupportsDisableLabel",
                "Supports explicit disable",
              )}
              checked={draft.supportsDisable}
              onChange={(event) =>
                updateReasoning(candidate, {
                  supportsDisable: event.target.checked,
                })
              }
              disabled={busy || saving}
              className="h-3.5 w-3.5 rounded border-border accent-primary"
            />
            <span>
              {t(
                "aiGatewayModelPoolReasoningSupportsDisableLabel",
                "Supports explicit disable",
              )}
            </span>
          </label>
        </div>
        {draft.profile === "chat_thinking_switch" ? (
          <p className="text-[10px] text-amber-600 dark:text-amber-400">
            {t(
              "aiGatewayModelPoolReasoningSwitchNoGradient",
              "This profile selects one intensity: every positive mapping sends the same switch, so there is no intensity gradient.",
            )}
          </p>
        ) : null}
        <div className="space-y-1">
          <div className="text-[10px] font-medium text-muted-foreground">
            {t("aiGatewayModelPoolReasoningMappingLabel", "Mapping matrix")}
          </div>
          <div className="grid gap-1.5 sm:grid-cols-5">
            {CANONICAL_REASONING_EFFORTS.map((effort) => (
              <label
                key={effort}
                className="flex flex-col gap-1 text-[10px] text-muted-foreground"
              >
                <span>{effort}</span>
                <select
                  data-testid={`model-pool-reasoning-mapping-${effort}`}
                  aria-label={`${t(
                    "aiGatewayModelPoolReasoningMappingLabel",
                    "Mapping matrix",
                  )}: ${effort}`}
                  value={draft.mappings[effort] ?? ""}
                  onChange={(event) =>
                    updateReasoning(candidate, {
                      mappings: {
                        ...draft.mappings,
                        [effort]: event.target.value,
                      },
                    })
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
                  {enumOptions.map((value) => (
                    <option key={`enum:${value}`} value={`enum:${value}`}>
                      {`enum:${value}`}
                    </option>
                  ))}
                  <option value="budget">
                    {t("aiGatewayModelPoolReasoningActionBudget", "Budget")}
                  </option>
                  <option value="disable">
                    {t("aiGatewayModelPoolReasoningActionDisable", "Disable")}
                  </option>
                </select>
                {draft.mappings[effort] === "budget" ? (
                  <input
                    type="number"
                    min={MODEL_POOL_MIN_REASONING_BUDGET}
                    step={1}
                    data-testid={`model-pool-reasoning-budget-${effort}`}
                    aria-label={`${t(
                      "aiGatewayModelPoolReasoningBudgetLabel",
                      "Reasoning budget",
                    )}: ${effort}`}
                    value={draft.budgetTokens[effort] ?? ""}
                    onChange={(event) =>
                      updateReasoning(candidate, {
                        budgetTokens: {
                          ...draft.budgetTokens,
                          [effort]: event.target.value,
                        },
                      })
                    }
                    disabled={busy || saving}
                    className={`${fieldClass} w-full`}
                  />
                ) : null}
              </label>
            ))}
          </div>
        </div>
      </div>
    );
  };

  const handleSave = async () => {
    if (busy || saving) return;
    const trimmedName = name.trim();
    const trimmedModel = model.trim();
    if (!trimmedName) {
      setError(t("aiGatewayModelPoolErrorNameRequired", "Enter a pool name."));
      return;
    }
    if (!trimmedModel) {
      setError(
        t("aiGatewayModelPoolErrorModelRequired", "Enter a public model ID."),
      );
      return;
    }
    // Reject a declaration the backend would refuse before any write, so the
    // whole draft is retained and the operator sees a localized reason.
    const selectedDrafts = selected.map((candidate) => ({
      candidate,
      draft:
        reasoningDrafts[candidate.key] ??
        reasoningDraftFrom(candidate.reasoning),
    }));
    const reasoningErrorKey = draftReasoningError(
      selectedDrafts.map((entry) => entry.draft),
    );
    if (reasoningErrorKey) {
      setError(t(reasoningErrorKey));
      return;
    }
    // A concrete default preference requires at least one currently valid
    // candidate mapping; `auto` omits the controls. The same save may not delete
    // the retained default's last declaration, while a retained, unchanged
    // declaration may survive an unavailable source in an unrelated edit.
    if (defaultEffort !== "auto") {
      const defaultUnmapped = () =>
        t(
          "aiGatewayModelPoolErrorReasoningDefaultUnmapped",
          "Invalid model pool reasoning policy: the default preference needs at least one valid candidate mapping.",
        );
      const declaresDefault = selectedDrafts.some(({ draft }) =>
        Boolean(draft.mappings[defaultEffort]),
      );
      if (!declaresDefault) {
        setError(defaultUnmapped());
        return;
      }
      const currentlyValid = selectedDrafts.some(({ candidate, draft }) =>
        modelPoolCandidateServesEffort(
          providers,
          {
            provider_id: candidate.provider_id,
            local_model: candidate.local_model,
            upstream_model: candidate.upstream_model,
            protocol: candidate.protocol,
            weight: candidate.weight,
            ...(hasDeclaredReasoning(draft)
              ? { reasoning: reasoningDraftPayload(draft) }
              : {}),
          },
          defaultEffort,
        ),
      );
      const existingDefault = pool
        ? resolveModelPoolDefaultEffort(pool)
        : null;
      const retained = pool != null && existingDefault === defaultEffort;
      const submittedSignature = effortDeclarationSignature(
        selectedDrafts.map(({ candidate, draft }) => ({
          key: candidate.key,
          action: draft.mappings[defaultEffort]
            ? optionValueToAction(
                draft.mappings[defaultEffort],
                draft,
                defaultEffort,
              )
            : null,
        })),
      );
      const declarationUnchanged =
        retained &&
        submittedSignature ===
          effortDeclarationSignature(
            (pool?.candidates ?? []).map((stored) => {
              const mapping = (stored.reasoning?.mappings ?? []).find(
                (item) => item.effort === defaultEffort,
              );
              return {
                key: modelPoolCandidateKey(stored),
                action: mapping ? mapping.action : null,
              };
            }),
          );
      if (!currentlyValid && (!retained || !declarationUnchanged)) {
        setError(defaultUnmapped());
        return;
      }
    }
    if (selected.length === 0) {
      setError(
        t(
          "aiGatewayModelPoolErrorSelectionRequired",
          "Select at least one source model.",
        ),
      );
      return;
    }
    const candidates: GatewayModelPoolCandidate[] = [];
    for (const candidate of selected) {
      const weight = parseWeight(weights[candidate.key] ?? "1");
      if (weight === null) {
        setError(
          t(
            "aiGatewayModelPoolErrorWeightInvalid",
            "Weight must be an integer between 1 and 100.",
          ),
        );
        return;
      }
      const draft =
        reasoningDrafts[candidate.key] ??
        reasoningDraftFrom(candidate.reasoning);
      candidates.push({
        provider_id: candidate.provider_id,
        local_model: candidate.local_model,
        upstream_model: candidate.upstream_model,
        protocol: candidate.protocol,
        weight,
        ...(hasDeclaredReasoning(draft)
          ? { reasoning: reasoningDraftPayload(draft) }
          : {}),
      });
    }
    setError(null);
    setSaving(true);
    try {
      await onSave({
        id: pool?.id || newModelPoolId(),
        name: trimmedName,
        model: trimmedModel,
        enabled: pool?.enabled ?? true,
        default_reasoning_effort: defaultEffort,
        candidates,
      });
      onOpenChange(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : errorToMessage(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        data-testid="model-pool-dialog"
        className="w-full sm:max-w-2xl lg:max-w-3xl max-h-[88vh] overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0"
      >
        <DialogHeader className="pl-6 pr-14 py-4 border-b bg-card/80 backdrop-blur-sm shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary">
              <Layers className="h-4.5 w-4.5" />
            </div>
            <div className="min-w-0">
              <DialogTitle className="truncate text-base font-semibold leading-5 text-foreground">
                {pool
                  ? t("aiGatewayModelPoolDialogEditTitle", "Edit model pool")
                  : t("aiGatewayModelPoolDialogTitle", "Add model pool")}
              </DialogTitle>
              <DialogDescription className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                {t(
                  "aiGatewayModelPoolDialogDesc",
                  "Select source models and assign each candidate an integer weight.",
                )}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-5 p-6">
          {/* Basic fields */}
          <div className="grid gap-4 sm:grid-cols-2">
            <label className="flex flex-col gap-1.5 text-xs font-medium text-foreground">
              <span>{t("aiGatewayModelPoolNameLabel", "Name")}</span>
              <input
                type="text"
                data-testid="model-pool-name-input"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t("aiGatewayModelPoolNamePlaceholder", "Free Modle")}
                disabled={busy || saving}
                className="h-9 rounded-lg border border-border/80 bg-background px-3 text-sm text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50"
              />
            </label>
            <label className="flex flex-col gap-1.5 text-xs font-medium text-foreground">
              <span>
                {t("aiGatewayModelPoolModelLabel", "Public model ID")}
              </span>
              <input
                type="text"
                data-testid="model-pool-model-input"
                value={model}
                onChange={(event) => setModel(event.target.value)}
                placeholder={t(
                  "aiGatewayModelPoolModelPlaceholder",
                  "free-model",
                )}
                disabled={busy || saving}
                className="h-9 rounded-lg border border-border/80 bg-background px-3 font-mono text-sm text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50"
              />
            </label>
          </div>

          {/* Pool default reasoning preference */}
          <label className="flex flex-col gap-1.5 text-xs font-medium text-foreground sm:max-w-xs">
            <span>
              {t("aiGatewayModelPoolReasoningDefaultLabel", "Reasoning default")}
            </span>
            <select
              data-testid="model-pool-reasoning-default"
              aria-label={t(
                "aiGatewayModelPoolReasoningDefaultLabel",
                "Reasoning default",
              )}
              value={defaultEffort}
              onChange={(event) => {
                setDefaultEffort(event.target.value);
                setError(null);
              }}
              disabled={busy || saving}
              className="h-9 rounded-lg border border-border/80 bg-background px-2 text-sm text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50"
            >
              <option value="auto">
                {t(
                  "aiGatewayModelPoolReasoningDefaultAuto",
                  "Provider default (auto)",
                )}
              </option>
              {CANONICAL_REASONING_EFFORTS.map((effort) => (
                <option key={effort} value={effort}>
                  {effort}
                </option>
              ))}
            </select>
          </label>

          {/* Live static coverage (config-derived) separated from runtime readiness */}
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[10px] text-muted-foreground">
            <span className="inline-flex items-center gap-1">
              <span>
                {t("aiGatewayModelPoolReasoningCoverageLabel", "Static coverage")}:
              </span>
              <span
                data-testid="model-pool-reasoning-static-coverage"
                className="font-medium text-foreground/85"
              >
                {t("aiGatewayModelPoolReasoningCoverageValue", {
                  mapped: coverageRow?.mapped_candidates ?? 0,
                  total: coverageRow?.total_candidates ?? selected.length,
                  defaultValue: "{{mapped}}/{{total}} candidates mapped",
                })}
              </span>
            </span>
            <span className="inline-flex items-center gap-1">
              <span>
                {t(
                  "aiGatewayModelPoolReasoningRuntimeReadinessLabel",
                  "Runtime readiness",
                )}
                :
              </span>
              <span
                data-testid="model-pool-reasoning-runtime-readiness"
                className="font-medium text-foreground/85"
              >
                {t("aiGatewayModelPoolReasoningRuntimeReadyCount", {
                  ready: readyCount,
                  total: selected.length,
                  defaultValue: "{{ready}}/{{total}} candidates ready",
                })}
              </span>
            </span>
          </div>

          {/* Source search */}
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/70" />
            <input
              type="search"
              data-testid="model-pool-source-search"
              aria-label={t("aiGatewayModelPoolSourceSearch", "Search source models")}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t(
                "aiGatewayModelPoolSourceSearchPlaceholder",
                "Search by provider or model",
              )}
              className="h-9 w-full rounded-lg border border-border/80 bg-background pl-9 pr-9 text-xs text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20"
            />
            {query ? (
              <button
                type="button"
                onClick={() => setQuery("")}
                aria-label={t("aiGatewayModelListClearSearch", "Clear search")}
                className="absolute right-2 top-1/2 -translate-y-1/2 rounded-full p-1 text-muted-foreground transition hover:bg-muted hover:text-foreground"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            ) : null}
          </div>

          {/* Source groups */}
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="text-xs font-semibold text-foreground">
                {t("aiGatewayModelPoolSourcesTitle", "Source models")}
              </div>
              {filteredCandidates.length > 0 ? (
                <button
                  type="button"
                  data-testid="model-pool-select-all-filtered"
                  onClick={toggleSelectAllFiltered}
                  className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs font-medium text-primary hover:bg-primary/10 transition-colors"
                >
                  <CheckSquare className="h-3.5 w-3.5" />
                  <span>
                    {allFilteredSelected
                      ? t(
                          "aiGatewayModelPoolDeselectAllFiltered",
                          "Deselect all ({{count}})",
                          { count: filteredCandidates.length },
                        )
                      : t(
                          "aiGatewayModelPoolSelectAllFiltered",
                          "Select all ({{count}})",
                          { count: filteredCandidates.length },
                        )}
                  </span>
                </button>
              ) : null}
            </div>
            {sourceGroups.length === 0 ? (
              <p className="rounded-lg border border-dashed bg-muted/20 px-3 py-4 text-center text-xs text-muted-foreground">
                {t("aiGatewayModelPoolNoSources", "No source models available.")}
              </p>
            ) : (
              sourceGroups.map((group) => {
                const visibleRows = group.rows.filter((row) =>
                  normalizedQuery
                    ? row.candidate.local_model
                        .toLowerCase()
                        .includes(normalizedQuery) ||
                      row.candidate.upstream_model
                        .toLowerCase()
                        .includes(normalizedQuery) ||
                      group.providerName.toLowerCase().includes(normalizedQuery)
                    : true,
                );
                const isCollapsed = Boolean(collapsed[group.providerId]);
                return (
                  <div
                    key={group.providerId}
                    className="rounded-xl border border-border/70 bg-card/60"
                  >
                    <button
                      type="button"
                      data-testid={`model-pool-source-group-${group.providerId}`}
                      aria-expanded={!isCollapsed}
                      onClick={() =>
                        setCollapsed((previous) => ({
                          ...previous,
                          [group.providerId]: !previous[group.providerId],
                        }))
                      }
                      className="flex w-full items-start justify-between gap-2 px-3 py-2 text-left"
                    >
                      <span className="inline-flex min-w-0 flex-1 items-center gap-2 text-xs font-semibold text-foreground">
                        <Server className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                        <span className="min-w-0 break-words">{group.providerName}</span>
                        {group.templateName ? (
                          <span
                            data-testid={`model-pool-source-template-badge-${group.providerId}`}
                            className="inline-flex items-center gap-1 rounded-md border border-indigo-500/25 bg-indigo-500/10 px-1.5 py-0.5 text-[10px] font-medium text-indigo-700 dark:text-indigo-300 shrink-0"
                            title={group.templateName}
                          >
                            <Sparkles className="h-2.5 w-2.5" />
                            <span>{group.templateName}</span>
                          </span>
                        ) : null}
                        <span className="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                          {group.rows.length}
                        </span>
                      </span>
                      <ChevronDown
                        className={`mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform ${
                          isCollapsed ? "-rotate-90" : ""
                        }`}
                      />
                    </button>
                    {!isCollapsed ? (
                      visibleRows.length === 0 ? (
                        <p className="px-3 pb-3 text-[11px] text-muted-foreground">
                          {t(
                            "aiGatewayModelListNoMatch",
                            "No models match the current search.",
                          )}
                        </p>
                      ) : (
                        <div className="space-y-1 px-2 pb-2">
                          {visibleRows.map(({ key, candidate }) => {
                            const checked = selectedKeys.has(key);
                            const state = resolveModelPoolCandidateState(
                              candidate,
                              providers,
                              statusByKey.get(key) ?? null,
                            );
                            const labelKey =
                              MODEL_POOL_AVAILABILITY_LABEL_KEYS[
                                state.availability
                              ];
                            const label = labelKey
                              ? t(labelKey)
                              : state.availability;
                            return (
                              <label
                                key={key}
                                className="flex cursor-pointer items-start gap-2.5 rounded-lg px-2 py-1.5 transition hover:bg-muted/50"
                              >
                                <input
                                  type="checkbox"
                                  checked={checked}
                                  onChange={() => toggleCandidate(candidate)}
                                  aria-label={`${group.providerName} ${candidate.local_model} ${candidate.upstream_model}`}
                                  disabled={busy || saving}
                                  className="mt-0.5 h-3.5 w-3.5 shrink-0 rounded border-border accent-primary"
                                />
                                <span className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5 text-xs">
                                  <span className="break-all font-medium text-foreground">
                                    {candidate.local_model}
                                  </span>
                                  <span className="text-muted-foreground/60">
                                    →
                                  </span>
                                  <code className="break-all rounded border border-border/40 bg-muted/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground">
                                    {candidate.upstream_model}
                                  </code>
                                  <span
                                    className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${protocolBadgeClass(
                                      candidate.protocol,
                                    )}`}
                                  >
                                    {protocolShortLabel(candidate.protocol)}
                                  </span>
                                  <span
                                    className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${
                                      state.ready
                                        ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                                        : "bg-amber-500/15 text-amber-700 dark:text-amber-400"
                                    }`}
                                  >
                                    {label}
                                  </span>
                                  {state.reason ? (
                                    <span className="break-words text-[10px] text-amber-600 dark:text-amber-400">
                                      {state.reason}
                                    </span>
                                  ) : null}
                                </span>
                              </label>
                            );
                          })}
                        </div>
                      )
                    ) : null}
                  </div>
                );
              })
            )}
          </div>

          {/* Selected candidates */}
          <div
            data-testid="model-pool-selected-list"
            className="space-y-2 rounded-xl border border-border/70 bg-muted/20 p-3"
          >
            <div className="text-xs font-semibold text-foreground">
              {t("aiGatewayModelPoolSelectedTitle", "Selected candidates")}
            </div>
            {selected.length === 0 ? (
              <p className="text-[11px] text-muted-foreground">
                {t(
                  "aiGatewayModelPoolSelectedEmpty",
                  "Select at least one source model.",
                )}
              </p>
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
                return (
                  <div
                    key={candidate.key}
                    className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border/70 bg-card/70 p-2.5 transition hover:bg-card/90"
                  >
                    {/* 左侧信息区：分为上下两行，层次分明 */}
                    <div className="min-w-0 flex-1 space-y-1.5">
                      {/* 第一行：模型映射与服务商来源 */}
                      <div className="flex flex-wrap items-center gap-2">
                        <span
                          className="font-semibold text-xs text-foreground truncate max-w-[200px]"
                          title={candidate.local_model}
                        >
                          {candidate.local_model}
                        </span>
                        <span className="text-muted-foreground/50 text-xs">→</span>
                        <code
                          className="rounded border border-border/50 bg-muted/70 px-1.5 py-0.5 font-mono text-[11px] text-foreground truncate max-w-[240px]"
                          title={candidate.upstream_model}
                        >
                          {candidate.upstream_model}
                        </code>

                        {/* 来源服务商与模板徽标 */}
                        <span className="inline-flex items-center gap-1.5 rounded-md bg-muted/60 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                          <Server className="h-2.5 w-2.5 text-muted-foreground/80 shrink-0" />
                          <span className="font-medium text-foreground/80 truncate max-w-[120px]">
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
                      </div>

                      {/* 第二行：协议、可用性状态与异常原因 */}
                      <div className="flex flex-wrap items-center gap-2 text-[10px]">
                        <span
                          className={`inline-flex items-center rounded-md px-1.5 py-0.5 font-medium ${protocolBadgeClass(
                            candidate.protocol,
                          )}`}
                        >
                          {protocolShortLabel(candidate.protocol)}
                        </span>
                        <span
                          className={`inline-flex items-center rounded-md px-1.5 py-0.5 font-medium ${
                            state.ready
                              ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                              : "bg-amber-500/15 text-amber-700 dark:text-amber-400"
                          }`}
                        >
                          {label}
                        </span>
                        {state.reason ? (
                          <span
                            className="truncate max-w-[280px] text-amber-600 dark:text-amber-400"
                            title={state.reason}
                          >
                            {state.reason}
                          </span>
                        ) : null}
                      </div>
                    </div>

                    {/* 右侧操作区：权重配置与删除操作 */}
                    <div className="flex items-center gap-2 shrink-0 self-center">
                      <div className="flex items-center gap-1.5 bg-muted/40 border border-border/50 rounded-lg px-2 py-1">
                        <span className="text-[10px] text-muted-foreground">
                          {t("aiGatewayModelPoolWeightLabel", "weight")}
                        </span>
                        <input
                          type="number"
                          min={1}
                          max={100}
                          step={1}
                          value={weights[candidate.key] ?? "1"}
                          onChange={(event) =>
                            setWeights((previous) => ({
                              ...previous,
                              [candidate.key]: event.target.value,
                            }))
                          }
                          aria-label={t("aiGatewayModelPoolWeightAria", {
                            model: candidate.local_model,
                            defaultValue: "{{model}} weight",
                          })}
                          disabled={busy || saving}
                          className="h-6 w-12 rounded border border-border/70 bg-background px-1 text-center font-mono text-xs text-foreground shadow-2xs outline-none focus:border-primary/60 focus-visible:ring-1 focus-visible:ring-primary/20 disabled:opacity-50"
                        />
                      </div>
                      <button
                        type="button"
                        onClick={() => removeCandidate(candidate.key)}
                        disabled={busy || saving}
                        aria-label={t("aiGatewayModelPoolRemoveAria", {
                          model: candidate.local_model,
                          defaultValue: "Remove {{model}}",
                        })}
                        className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition disabled:opacity-50"
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                    </div>
                    {reasoningEditor(candidate)}
                  </div>
                );
              })
            )}
          </div>

          {error ? (
            <div
              role="alert"
              data-testid="model-pool-error"
              className="rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-xs text-destructive"
            >
              {error}
            </div>
          ) : null}
        </div>

        <DialogFooter className="flex flex-row items-center justify-end gap-2 border-t px-6 py-4 shrink-0">
          <button
            type="button"
            data-testid="model-pool-cancel"
            onClick={() => onOpenChange(false)}
            disabled={busy || saving}
            className="inline-flex h-8 items-center rounded-lg border border-border/80 bg-background px-3 text-xs font-medium text-foreground transition hover:bg-muted disabled:opacity-50"
          >
            {t("aiGatewayModelPoolCancel", "Cancel")}
          </button>
          <button
            type="button"
            data-testid="model-pool-save"
            onClick={() => void handleSave()}
            disabled={busy || saving}
            className="inline-flex h-8 items-center rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-xs transition hover:bg-primary/90 disabled:opacity-50"
          >
            {t("aiGatewayModelPoolSave", "Save")}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
