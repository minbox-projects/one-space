import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  CheckSquare,
  ChevronDown,
  ChevronUp,
  Layers,
  Search,
  Server,
  SlidersHorizontal,
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
import { useToast } from "@/components/ToastProvider";
import { errorToMessage } from "@/lib/messages";
import {
  aggregateModels,
  autoBindCandidateReasoning,
  CANONICAL_REASONING_EFFORTS,
  expandModelPoolFallbackModel,
  MODEL_POOL_AVAILABILITY_LABEL_KEYS,
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

/**
 * The candidate-local reasoning draft. It stores only the operator's chosen
 * bindings — public effort -> stable mapping option — and never an editable
 * profile, declared value, budget range, disable toggle or budget token. The
 * immutable stored `candidate.reasoning` remains the read-only base the payload
 * is derived from, so no manual-parameter state ever goes dormant here.
 */
type ReasoningDraft = {
  /** public effort -> mapping option (`""` unmapped, `enum:<v>`, `budget:<tokens>`, `disable`). */
  mappings: Record<string, string>;
};

/** Convert one stored typed action into its stable binding option value. */
function actionToOptionValue(action: ModelPoolReasoningAction): string {
  if (action?.kind === "enum" && typeof action.value === "string") {
    return `enum:${action.value}`;
  }
  if (action?.kind === "budget") {
    return typeof action.budget_tokens === "number"
      ? `budget:${action.budget_tokens}`
      : "budget";
  }
  if (action?.kind === "disable") return "disable";
  return "";
}

/** Seed a draft from the stored declaration's bindings only. */
function reasoningDraftFrom(
  reasoning: GatewayModelPoolReasoning | null | undefined,
): ReasoningDraft {
  const mappings: Record<string, string> = {};
  for (const mapping of reasoning?.mappings ?? []) {
    const option = actionToOptionValue(mapping.action);
    if (option !== "") mappings[mapping.effort] = option;
  }
  return { mappings };
}

/** The bounded adapter a candidate protocol uses for its first explicit binding. */
function defaultProfileForProtocol(protocol: GatewayUpstreamProtocol): string {
  return protocol === "anthropic_messages"
    ? "anthropic_effort_inherit"
    : "openai_effort";
}

/** The reserved disable/omission native token; never a positive binding target. */
function isReservedNoneEffort(value: string): boolean {
  return value.trim().toLowerCase() === "none";
}

/** The exact source mapping one candidate references, if it currently resolves. */
function findCandidateMapping(
  providers: GatewayUpstreamProvider[],
  candidate: Pick<
    GatewayModelPoolCandidate,
    "provider_id" | "local_model" | "upstream_model" | "protocol"
  >,
) {
  const provider = providers.find((item) => item.id === candidate.provider_id);
  if (!provider) return null;
  return (
    (provider.mappings ?? []).find(
      (mapping) =>
        mapping.local_model.trim() === candidate.local_model &&
        mapping.upstream_model.trim() === candidate.upstream_model &&
        (mapping.protocol ?? provider.protocol ?? "chat_completions") ===
          candidate.protocol,
    ) ?? null
  );
}

/**
 * The native reasoning levels offered for one candidate: the exact source
 * mapping's declared `reasoning_efforts`, unioned with the candidate's stored
 * declared values, its stored enum actions and its currently bound enums.
 * Everything is trimmed and de-duplicated without rewriting its case, and no
 * sibling model, other provider or name-based profile guess contributes.
 */
function candidateNativeEfforts(
  providers: GatewayUpstreamProvider[],
  candidate: DraftCandidate,
  draft: ReasoningDraft,
): string[] {
  const mapping = findCandidateMapping(providers, candidate);
  const storedEnum = (candidate.reasoning?.mappings ?? [])
    .filter(
      (item) => item.action?.kind === "enum" && typeof item.action.value === "string",
    )
    .map((item) => item.action.value as string);
  const boundEnum = Object.values(draft.mappings)
    .filter((option) => option.startsWith("enum:"))
    .map((option) => option.slice("enum:".length));
  const normalized: string[] = [];
  for (const value of [
    ...(mapping?.reasoning_efforts ?? []),
    ...(candidate.reasoning?.values ?? []),
    ...storedEnum,
    ...boundEnum,
  ]) {
    const trimmed = value.trim();
    if (trimmed !== "" && !normalized.includes(trimmed)) normalized.push(trimmed);
  }
  return normalized;
}

/** Convert one binding option back into its typed native action. */
function optionValueToAction(option: string): ModelPoolReasoningAction | null {
  if (option.startsWith("enum:")) {
    return { kind: "enum", value: option.slice("enum:".length) };
  }
  if (option.startsWith("budget:")) {
    const tokens = Number(option.slice("budget:".length).trim());
    return Number.isInteger(tokens)
      ? { kind: "budget", budget_tokens: tokens }
      : { kind: "budget" };
  }
  if (option === "budget") return { kind: "budget" };
  if (option === "disable") return { kind: "disable" };
  return null;
}

/** The distinct native budget tokens a stored declaration offers as rebind targets. */
function storedBudgetTokens(
  reasoning: GatewayModelPoolReasoning | null | undefined,
): number[] {
  const tokens: number[] = [];
  for (const mapping of reasoning?.mappings ?? []) {
    const value = mapping.action?.budget_tokens;
    if (mapping.action?.kind === "budget" && typeof value === "number") {
      if (!tokens.includes(value)) tokens.push(value);
    }
  }
  return tokens;
}

/** Whether a stored declaration carries any metadata at all. */
function hasStoredReasoning(
  reasoning: GatewayModelPoolReasoning | null | undefined,
): boolean {
  if (!reasoning) return false;
  return (
    (reasoning.profile ?? "") !== "" ||
    (reasoning.values ?? []).length > 0 ||
    reasoning.budget != null ||
    reasoning.supports_disable === true ||
    (reasoning.mappings ?? []).length > 0
  );
}

/**
 * Derive a candidate's persisted reasoning payload from its immutable stored
 * base and the operator's chosen bindings. The stored `profile`, `values`,
 * `budget` and `supports_disable` are retained as read-only inputs, so editing
 * never rewrites them; the payload only appends a chosen native enum value to
 * `values` when needed and sets `supports_disable` when the operator binds `none`
 * to an explicit disable. An untouched new candidate (no stored declaration and
 * no binding) emits nothing, so clearing every binding never leaves an orphaned
 * declaration.
 */
function deriveReasoning(
  base: GatewayModelPoolReasoning | null | undefined,
  draft: ReasoningDraft,
  protocol: GatewayUpstreamProtocol,
): GatewayModelPoolReasoning | undefined {
  const bindings = CANONICAL_REASONING_EFFORTS.flatMap((effort) => {
    const action = optionValueToAction(draft.mappings[effort] ?? "");
    return action ? [{ effort, action }] : [];
  });
  if (!hasStoredReasoning(base) && bindings.length === 0) return undefined;

  const values = [...(base?.values ?? [])];
  for (const binding of bindings) {
    if (binding.action.kind !== "enum" || typeof binding.action.value !== "string") {
      continue;
    }
    if (!values.includes(binding.action.value)) values.push(binding.action.value);
  }
  const storedProfile = base?.profile ?? "";
  return {
    profile:
      storedProfile !== "" ? storedProfile : defaultProfileForProtocol(protocol),
    values,
    ...(base?.budget != null ? { budget: base.budget } : {}),
    supports_disable:
      base?.supports_disable === true ||
      bindings.some((binding) => binding.action.kind === "disable"),
    mappings: bindings,
  };
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
  /** Current and retired pool aliases that a fallback model must not reuse. */
  aliases?: string[];
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
  aliases = [],
  templates = [],
  pool = null,
  poolStatus = null,
  busy = false,
  onSave,
}: ModelPoolDialogProps) {
  const { t } = useTranslation();
  const { pushToast } = useToast();

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
  // Ordered fallback gateway model ids. The list never shares regular weights
  // or primary scheduling, and each id expands to its provider mappings later.
  const [fallbackModels, setFallbackModels] = useState<string[]>(() =>
    (pool?.fallback_models ?? []).map((model) => model.trim()).filter(Boolean),
  );
  const [fallbackQuery, setFallbackQuery] = useState("");
  const [fallbackDropdownOpen, setFallbackDropdownOpen] = useState(false);
  const fallbackComboboxRef = useRef<HTMLDivElement>(null);
  const fallbackInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!fallbackDropdownOpen) return;
    function handleClickOutside(event: MouseEvent) {
      if (
        fallbackComboboxRef.current &&
        !fallbackComboboxRef.current.contains(event.target as Node)
      ) {
        setFallbackDropdownOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [fallbackDropdownOpen]);
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
  // 单个候选模型的推理策略面板展开状态（默认收起）
  const [expandedReasoning, setExpandedReasoning] = useState<
    Record<string, boolean>
  >({});

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
    // The immutable stored base for each already-saved candidate, indexed by its
    // full source identity. Re-selecting a previously deselected source row must
    // reattach that exact base (profile/values/budget/supports_disable) rather
    // than the bare source tuple, so the stored manual/adaptive metadata survives
    // a deselect/reselect round-trip. Only the exact identity matches: no sibling
    // model or other provider ever contributes, and new sources stay unknown.
    const storedReasoningByKey = new Map<string, GatewayModelPoolReasoning>();
    for (const candidate of pool?.candidates ?? []) {
      if (candidate.reasoning) {
        storedReasoningByKey.set(
          modelPoolCandidateKey(candidate),
          candidate.reasoning,
        );
      }
    }
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
            const key = modelPoolCandidateKey(candidate);
            const storedReasoning = storedReasoningByKey.get(key);
            return {
              key,
              candidate: storedReasoning
                ? { ...candidate, reasoning: storedReasoning }
                : candidate,
            };
          })
          .filter((row): row is { key: string; candidate: GatewayModelPoolCandidate } =>
            row !== null,
          ),
      };
    });
    return groups;
  }, [activeProviders, templates, pool]);

  const normalizedQuery = query.trim().toLowerCase();

  const selectedKeys = useMemo(
    () => new Set(selected.map((candidate) => candidate.key)),
    [selected],
  );

  // Current and retired pool aliases can never be selected as fallback models.
  const aliasModels = useMemo(
    () =>
      new Set(
        aliases.map((alias) => alias.trim()).filter((alias) => alias !== ""),
      ),
    [aliases],
  );

  // Options come from the aggregated enabled gateway models: model-level ids,
  // never provider-specific mapping labels. Already added ids and aliases are
  // excluded; a regular candidate id remains a legal fallback selection.
  const fallbackModelOptions = useMemo(() => {
    const added = new Set(fallbackModels);
    return aggregateModels(activeProviders)
      .filter((entry) => !aliasModels.has(entry.model) && !added.has(entry.model))
      .map((entry) => entry.model);
  }, [activeProviders, aliasModels, fallbackModels]);

  const filteredFallbackModelOptions = useMemo(() => {
    const trimmed = fallbackQuery.trim().toLowerCase();
    if (!trimmed) return fallbackModelOptions;
    return fallbackModelOptions.filter((item) =>
      item.toLowerCase().includes(trimmed),
    );
  }, [fallbackModelOptions, fallbackQuery]);

  const filteredFallbackModels = useMemo(() => {
    const trimmed = fallbackQuery.trim().toLowerCase();
    if (!trimmed) return fallbackModels;
    return fallbackModels.filter((item) =>
      item.toLowerCase().includes(trimmed),
    );
  }, [fallbackModels, fallbackQuery]);

  // Distinct enabled provider count per aggregated model, also used for stored
  // ids that are no longer offered by the add control.
  const fallbackProviderCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const entry of aggregateModels(activeProviders)) {
      counts.set(
        entry.model,
        new Set(entry.providers.map((item) => item.providerId)).size,
      );
    }
    return counts;
  }, [activeProviders]);

  const addFallbackModel = (model: string) => {
    const trimmed = model.trim();
    if (!trimmed) return;
    setFallbackModels((previous) =>
      previous.includes(trimmed) ? previous : [...previous, trimmed],
    );
    setError(null);
  };

  const moveFallbackModel = (index: number, delta: number) => {
    setFallbackModels((previous) => {
      const target = index + delta;
      if (index < 0 || index >= previous.length || target < 0 || target >= previous.length) {
        return previous;
      }
      const next = [...previous];
      const [moved] = next.splice(index, 1);
      next.splice(target, 0, moved);
      return next;
    });
    setError(null);
  };

  const removeFallbackModel = (model: string) => {
    setFallbackModels((previous) => previous.filter((item) => item !== model));
    setError(null);
  };

  const clearFallbackModels = () => {
    setFallbackModels([]);
    setError(null);
  };

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

  // Every provider mapping serving the current ordered fallback list, with its
  // reasoning auto-derived from the exact mapping and effective protocol.
  const expandedFallbackCandidates = useMemo(
    () =>
      fallbackModels.flatMap((model) =>
        expandModelPoolFallbackModel(providers, model),
      ),
    [fallbackModels, providers],
  );

  // Live static coverage derived from the current binding drafts and the
  // current provider list; readiness stays a separate, key/health-aware count.
  // The denominator spans the configured union of regular candidates and every
  // expanded fallback source, while runtime readiness below remains regular-only.
  const draftReasoningCatalog = useMemo(() => {
    const candidates: GatewayModelPoolCandidate[] = selected.map((candidate) => {
      const draft =
        reasoningDrafts[candidate.key] ??
        reasoningDraftFrom(candidate.reasoning);
      const reasoning = deriveReasoning(
        candidate.reasoning,
        draft,
        candidate.protocol,
      );
      return { ...candidate, ...(reasoning ? { reasoning } : {}) };
    });
    candidates.push(...expandedFallbackCandidates);
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
  }, [expandedFallbackCandidates, providers, reasoningDrafts, selected]);

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

      // 对新增候选项进行自动绑定并统计未匹配模型
      const draftsUpdate: Record<string, ReasoningDraft> = {};
      const unmatchedNames: string[] = [];
      for (const candidate of newItems) {
        const key = candidate.key;
        if (reasoningDrafts[key] === undefined) {
          const hasStoredMappings =
            (candidate.reasoning?.mappings ?? []).length > 0;
          if (hasStoredMappings) {
            draftsUpdate[key] = reasoningDraftFrom(candidate.reasoning);
          } else {
            const { mappings, hasMatch } = autoBindCandidateReasoning(
              candidate,
              providers,
            );
            draftsUpdate[key] = { mappings };
            if (!hasMatch) {
              unmatchedNames.push(candidate.local_model);
            }
          }
        }
      }

      setReasoningDrafts((previous) => ({
        ...previous,
        ...draftsUpdate,
      }));

      if (unmatchedNames.length > 0) {
        const message =
          unmatchedNames.length === 1
            ? t("aiGatewayModelPoolNoMatchingReasoningWarning", {
                model: unmatchedNames[0],
                defaultValue:
                  "No matching reasoning levels found for {{model}}, skipped binding. Please configure manually.",
              })
            : t("aiGatewayModelPoolMultipleNoMatchingReasoningWarning", {
                count: unmatchedNames.length,
                models: unmatchedNames.join(", "),
                defaultValue:
                  "{{count}} models have no matching reasoning levels ({{models}}), skipped binding. Please configure manually.",
              });
        pushToast({
          title: t(
            "aiGatewayModelPoolAutoBindingToastTitle",
            "Reasoning Binding",
          ),
          description: message,
          kind: "warning",
        });
      }
    }
    setError(null);
  };

  const toggleCandidate = (candidate: GatewayModelPoolCandidate) => {
    const key = modelPoolCandidateKey(candidate);
    const isAdding = !selected.some((item) => item.key === key);
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

    if (isAdding) {
      const existingDraft = reasoningDrafts[key];
      const hasStoredMappings =
        (candidate.reasoning?.mappings ?? []).length > 0;
      if (existingDraft === undefined) {
        if (hasStoredMappings) {
          setReasoningDrafts((previous) => ({
            ...previous,
            [key]: reasoningDraftFrom(candidate.reasoning),
          }));
        } else {
          const { mappings, hasMatch } = autoBindCandidateReasoning(
            candidate,
            providers,
          );
          setReasoningDrafts((previous) => ({
            ...previous,
            [key]: { mappings },
          }));
          if (!hasMatch) {
            pushToast({
              title: t(
                "aiGatewayModelPoolAutoBindingToastTitle",
                "Reasoning Binding",
              ),
              description: t("aiGatewayModelPoolNoMatchingReasoningWarning", {
                model: candidate.local_model,
                defaultValue:
                  "No matching reasoning levels found for {{model}}, skipped binding. Please configure manually.",
              }),
              kind: "warning",
            });
          }
        }
      }
    }

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
    const base = candidate.reasoning ?? null;
    // Native choices come from this candidate's exact source mapping, unioned
    // with its own stored values and bound enums so legacy declarations stay
    // representable; nothing is borrowed from a sibling model, another provider
    // or a name-based profile guess.
    const nativeEfforts = candidateNativeEfforts(providers, candidate, draft);
    const nativePositiveEfforts = nativeEfforts.filter(
      (value) => !isReservedNoneEffort(value),
    );
    // The stored (immutable) profile decides which native targets are valid; a
    // binding never rewrites it, and a new candidate picks its adapter only at
    // derivation time.
    const storedProfile = base?.profile ?? "";
    const isManualProfile = storedProfile === "anthropic_manual_budget";
    const isSwitchProfile = storedProfile === "chat_thinking_switch";
    const budgetTokens = storedBudgetTokens(base);
    const noneOffersDisable =
      nativeEfforts.includes("none") || base?.supports_disable === true;
    const fieldClass =
      "h-8 rounded-lg border border-border/80 bg-background px-2 text-xs text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50";
    const isExpanded = Boolean(expandedReasoning[candidate.key]);

    const updateBinding = (effort: string, option: string) => {
      updateReasoning(candidate, {
        mappings: { ...draft.mappings, [effort]: option },
      });
    };

    return (
      <div
        data-testid={`model-pool-reasoning-editor-${candidate.provider_id}-${candidate.local_model}`}
        className="w-full rounded-xl border border-border/60 bg-muted/25 overflow-hidden transition-all"
      >
        {/* 顶部标题栏兼折叠切换条 */}
        <button
          type="button"
          aria-expanded={isExpanded}
          onClick={() =>
            setExpandedReasoning((previous) => ({
              ...previous,
              [candidate.key]: !previous[candidate.key],
            }))
          }
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
                  // `none` is the explicit disable: it never offers an enum or
                  // a budget, and only where the source declares `none` or the
                  // stored declaration already supports it.
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
                  // A manual-budget candidate keeps exactly its original stored
                  // distinct budgets as immutable rebind targets; source native
                  // levels are not representable as enums and no new token or
                  // range may be typed.
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
                  // A switch profile's only positive native value is `enabled`,
                  // supplied by the adapter even when the source metadata omits
                  // it; no other native level is a valid switch target.
                  bindingOptions.push({
                    value: "enum:enabled",
                    label: "enabled",
                  });
                } else {
                  // Enum-capable profiles and an unset/unknown declaration keep
                  // offering this candidate's exact native levels.
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
    // Derive each candidate's persisted reasoning from its immutable stored base
    // and the operator's chosen bindings; the whole draft is retained on any
    // rejection and the operator sees a localized reason.
    const selectedDrafts = selected.map((candidate) => {
      const draft =
        reasoningDrafts[candidate.key] ??
        reasoningDraftFrom(candidate.reasoning);
      return {
        candidate,
        draft,
        reasoning: deriveReasoning(
          candidate.reasoning,
          draft,
          candidate.protocol,
        ),
      };
    });
    // A concrete default preference requires at least one currently valid
    // candidate mapping across the configured union of regular drafts and every
    // expanded fallback source; `auto` omits the controls. The same save may not
    // delete the retained default's last declaration, while a retained,
    // unchanged declaration may survive an unavailable source in an unrelated
    // edit.
    if (defaultEffort !== "auto") {
      const defaultUnmapped = () =>
        t(
          "aiGatewayModelPoolErrorReasoningDefaultUnmapped",
          "Invalid model pool reasoning policy: the default preference needs at least one valid candidate mapping.",
        );
      const declaresDefault =
        selectedDrafts.some(({ draft }) =>
          Boolean(draft.mappings[defaultEffort]),
        ) ||
        expandedFallbackCandidates.some((candidate) =>
          (candidate.reasoning?.mappings ?? []).some(
            (mapping) => mapping.effort === defaultEffort,
          ),
        );
      if (!declaresDefault) {
        setError(defaultUnmapped());
        return;
      }
      const unionCandidates: GatewayModelPoolCandidate[] = [
        ...selectedDrafts.map(({ candidate, reasoning }) => ({
          provider_id: candidate.provider_id,
          local_model: candidate.local_model,
          upstream_model: candidate.upstream_model,
          protocol: candidate.protocol,
          weight: candidate.weight,
          ...(reasoning ? { reasoning } : {}),
        })),
        ...expandedFallbackCandidates,
      ];
      const currentlyValid = unionCandidates.some((candidate) =>
        modelPoolCandidateServesEffort(providers, candidate, defaultEffort),
      );
      const existingDefault = pool
        ? resolveModelPoolDefaultEffort(pool)
        : null;
      const retained = pool != null && existingDefault === defaultEffort;
      const submittedSignature = effortDeclarationSignature([
        ...selectedDrafts.map(({ candidate, draft }) => ({
          key: candidate.key,
          action: draft.mappings[defaultEffort]
            ? optionValueToAction(draft.mappings[defaultEffort])
            : null,
        })),
        ...expandedFallbackCandidates.map((candidate) => {
          const mapping = (candidate.reasoning?.mappings ?? []).find(
            (item) => item.effort === defaultEffort,
          );
          return {
            key: modelPoolCandidateKey(candidate),
            action: mapping ? mapping.action : null,
          };
        }),
      ]);
      const storedPoolCandidates = [
        ...(pool?.candidates ?? []),
        ...(pool?.fallback_models ?? []).flatMap((model) =>
          expandModelPoolFallbackModel(providers, model),
        ),
      ];
      const declarationUnchanged =
        retained &&
        submittedSignature ===
          effortDeclarationSignature(
            storedPoolCandidates.map((stored) => {
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
      const reasoning = deriveReasoning(
        candidate.reasoning,
        draft,
        candidate.protocol,
      );
      candidates.push({
        provider_id: candidate.provider_id,
        local_model: candidate.local_model,
        upstream_model: candidate.upstream_model,
        protocol: candidate.protocol,
        weight,
        ...(reasoning ? { reasoning } : {}),
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
        fallback_models: fallbackModels,
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

          {/* Ordered fallback gateway models, separate from regular selection */}
          <div className="space-y-2 rounded-xl border border-border/70 bg-muted/20 p-3">
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0 space-y-0.5">
                <div className="text-xs font-semibold text-foreground">
                  {t("aiGatewayModelPoolFallbackLabel", "Fallback models")}
                </div>
                <p className="text-[10px] leading-relaxed text-muted-foreground">
                  {t(
                    "aiGatewayModelPoolFallbackHint",
                    "Ordered fallback models are used only after every regular candidate is exhausted.",
                  )}
                </p>
              </div>
              {fallbackModels.length > 0 ? (
                <button
                  type="button"
                  onClick={clearFallbackModels}
                  disabled={busy || saving}
                  aria-label={t(
                    "aiGatewayModelPoolFallbackClear",
                    "Clear fallback models",
                  )}
                  className="inline-flex h-7 shrink-0 items-center rounded-md border border-border/80 bg-background px-2 text-[11px] font-medium text-foreground transition hover:bg-muted disabled:opacity-50"
                >
                  {t(
                    "aiGatewayModelPoolFallbackClear",
                    "Clear fallback models",
                  )}
                </button>
              ) : null}
            </div>

            <div className="flex flex-col gap-1.5 text-xs font-medium text-foreground">
              <div className="flex items-center justify-between">
                <span>
                  {t(
                    "aiGatewayModelPoolFallbackAddLabel",
                    "Add fallback model",
                  )}
                </span>
                {fallbackQuery.trim() ? (
                  <span
                    data-testid="model-pool-fallback-match-count"
                    className="text-[10px] text-muted-foreground"
                  >
                    {t(
                      "aiGatewayModelPoolFallbackMatchCount",
                      "{{count}} fallback models matching",
                      { count: filteredFallbackModelOptions.length },
                    )}
                  </span>
                ) : null}
              </div>

              {/* Searchable Combobox for selecting & adding fallback models */}
              <div ref={fallbackComboboxRef} className="relative w-full">
                <div className="relative flex items-center">
                  <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/70" />
                  <input
                    ref={fallbackInputRef}
                    type="search"
                    data-testid="model-pool-fallback-search"
                    aria-label={t(
                      "aiGatewayModelPoolFallbackComboboxAria",
                      "Search fallback models to add",
                    )}
                    aria-expanded={fallbackDropdownOpen}
                    value={fallbackQuery}
                    onFocus={() => setFallbackDropdownOpen(true)}
                    onChange={(event) => {
                      setFallbackQuery(event.target.value);
                      if (!fallbackDropdownOpen) setFallbackDropdownOpen(true);
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") {
                        setFallbackDropdownOpen(false);
                      } else if (
                        event.key === "Enter" &&
                        filteredFallbackModelOptions.length > 0
                      ) {
                        event.preventDefault();
                        addFallbackModel(filteredFallbackModelOptions[0]);
                        setFallbackQuery("");
                        setFallbackDropdownOpen(false);
                      }
                    }}
                    placeholder={t(
                      "aiGatewayModelPoolFallbackComboboxPlaceholder",
                      "Search or select fallback model to add...",
                    )}
                    disabled={busy || saving}
                    className="h-9 w-full rounded-lg border border-border/80 bg-background pl-9 pr-16 text-xs text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50"
                  />
                  <div className="absolute right-1.5 flex items-center gap-0.5">
                    {fallbackQuery ? (
                      <button
                        type="button"
                        tabIndex={-1}
                        onClick={() => {
                          setFallbackQuery("");
                          fallbackInputRef.current?.focus();
                        }}
                        aria-label={t("aiGatewayModelListClearSearch", "Clear search")}
                        className="rounded-full p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
                      >
                        <X className="h-3 w-3" />
                      </button>
                    ) : null}
                    <button
                      type="button"
                      tabIndex={-1}
                      data-testid="model-pool-fallback-combobox-toggle"
                      onClick={() =>
                        setFallbackDropdownOpen((previous) => !previous)
                      }
                      aria-label={t(
                        "aiGatewayModelPoolFallbackAddAria",
                        "Add fallback model",
                      )}
                      disabled={busy || saving}
                      className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
                    >
                      <ChevronDown
                        className={`h-4 w-4 transition-transform duration-150 ${
                          fallbackDropdownOpen ? "rotate-180" : ""
                        }`}
                      />
                    </button>
                  </div>
                </div>

                {/* Dropdown menu */}
                {fallbackDropdownOpen && (
                  <div
                    role="listbox"
                    data-testid="model-pool-fallback-combobox-menu"
                    className="absolute left-0 top-full z-40 mt-1 max-h-56 w-full overflow-y-auto rounded-lg border border-border/80 bg-popover p-1 shadow-lg"
                  >
                    {filteredFallbackModelOptions.length > 0 ? (
                      filteredFallbackModelOptions.map((candidateModel) => {
                        const providerCount =
                          fallbackProviderCounts.get(candidateModel) ?? 0;
                        return (
                          <div
                            key={candidateModel}
                            role="option"
                            aria-selected={false}
                            data-testid={`model-pool-fallback-option-${candidateModel}`}
                            onClick={() => {
                              addFallbackModel(candidateModel);
                              setFallbackQuery("");
                              setFallbackDropdownOpen(false);
                            }}
                            className="flex cursor-pointer items-center justify-between gap-2 rounded-md px-2.5 py-1.5 text-xs text-foreground hover:bg-muted transition-colors"
                          >
                            <span className="truncate font-mono font-medium">
                              {candidateModel}
                            </span>
                            <span className="shrink-0 text-[10px] text-muted-foreground">
                              {t(
                                "aiGatewayModelPoolFallbackProviderCount",
                                "{{count}} providers",
                                { count: providerCount },
                              )}
                            </span>
                          </div>
                        );
                      })
                    ) : (
                      <div
                        data-testid="model-pool-fallback-combobox-empty"
                        className="px-3 py-2 text-center text-xs text-muted-foreground"
                      >
                        {t(
                          "aiGatewayModelPoolFallbackNoMatch",
                          "No matching fallback models",
                        )}
                      </div>
                    )}
                  </div>
                )}

                {/* Backward compatibility standard select */}
                <select
                  data-testid="model-pool-fallback-add"
                  aria-label={t(
                    "aiGatewayModelPoolFallbackAddAria",
                    "Add fallback model",
                  )}
                  value=""
                  onChange={(event) => {
                    addFallbackModel(event.target.value);
                  }}
                  disabled={busy || saving}
                  className="sr-only"
                >
                  <option value="">
                    {t(
                      "aiGatewayModelPoolFallbackAddPlaceholder",
                      "Add fallback model",
                    )}
                  </option>
                  {fallbackModelOptions.map((model) => (
                    <option key={model} value={model}>
                      {model}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            {fallbackModels.length === 0 ? (
              <p className="rounded-lg border border-dashed border-border/70 bg-card/40 px-3 py-3 text-center text-[11px] text-muted-foreground">
                {t(
                  "aiGatewayModelPoolFallbackEmpty",
                  "No fallback models configured.",
                )}
              </p>
            ) : filteredFallbackModels.length === 0 ? (
              <p
                data-testid="model-pool-fallback-empty-filter"
                className="rounded-lg border border-dashed border-border/70 bg-card/40 px-3 py-3 text-center text-[11px] text-muted-foreground"
              >
                {t(
                  "aiGatewayModelPoolFallbackNoConfiguredMatch",
                  "No configured fallback models match the filter.",
                )}
              </p>
            ) : (
              <ul
                data-testid="model-pool-fallback-list"
                className="space-y-1.5"
              >
                {filteredFallbackModels.map((model) => {
                  const rawIndex = fallbackModels.indexOf(model);
                  const providerCount =
                    fallbackProviderCounts.get(model) ?? 0;
                  return (
                    <li
                      key={model}
                      role="listitem"
                      className="flex items-center justify-between gap-2 rounded-lg border border-border/60 bg-card/70 px-2.5 py-2"
                    >
                      <div className="min-w-0">
                        <div className="break-all font-mono text-xs font-medium text-foreground">
                          {model}
                        </div>
                        <div
                          className={`text-[10px] ${
                            providerCount > 0
                              ? "text-muted-foreground"
                              : "text-amber-700 dark:text-amber-400"
                          }`}
                        >
                          {providerCount > 0
                            ? t(
                                "aiGatewayModelPoolFallbackProviderCount",
                                "{{count}} providers",
                                { count: providerCount },
                              )
                            : t(
                                "aiGatewayModelPoolFallbackUnavailable",
                                "Unavailable — no provider mapping",
                              )}
                        </div>
                      </div>
                      <div className="flex shrink-0 items-center gap-0.5">
                        <button
                          type="button"
                          onClick={() => moveFallbackModel(rawIndex, -1)}
                          disabled={busy || saving || rawIndex === 0}
                          aria-label={t(
                            "aiGatewayModelPoolFallbackMoveUpAria",
                            {
                              model,
                              defaultValue: "Move fallback {{model}} up",
                            },
                          )}
                          className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-40"
                        >
                          <ChevronUp className="h-3.5 w-3.5" />
                        </button>
                        <button
                          type="button"
                          onClick={() => moveFallbackModel(rawIndex, 1)}
                          disabled={
                            busy ||
                            saving ||
                            rawIndex === fallbackModels.length - 1
                          }
                          aria-label={t(
                            "aiGatewayModelPoolFallbackMoveDownAria",
                            {
                              model,
                              defaultValue: "Move fallback {{model}} down",
                            },
                          )}
                          className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-40"
                        >
                          <ChevronDown className="h-3.5 w-3.5" />
                        </button>
                        <button
                          type="button"
                          onClick={() => removeFallbackModel(model)}
                          disabled={busy || saving}
                          aria-label={t(
                            "aiGatewayModelPoolFallbackRemoveAria",
                            {
                              model,
                              defaultValue: "Remove fallback {{model}}",
                            },
                          )}
                          className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>

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
            className="space-y-3 rounded-xl border border-border/70 bg-muted/20 p-3"
          >
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <span className="text-xs font-semibold text-foreground">
                  {t("aiGatewayModelPoolSelectedTitle", "Selected candidates")}
                </span>
                <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
                  {selected.length}
                </span>
              </div>
            </div>
            {selected.length === 0 ? (
              <p className="rounded-lg border border-dashed border-border/70 bg-card/40 px-3 py-6 text-center text-xs text-muted-foreground">
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
                    className="flex flex-col rounded-xl border border-border/70 bg-card/70 p-3 transition hover:bg-card/90 space-y-2.5"
                  >
                    {/* 顶部主行：模型映射标识与右上角操作区 */}
                    <div className="flex items-start justify-between gap-3">
                      {/* 模型映射标识：本地模型 -> 上游模型 */}
                      <div className="min-w-0 flex-1 flex flex-wrap items-center gap-2">
                        <span
                          className="font-semibold text-sm text-foreground break-all"
                          title={candidate.local_model}
                        >
                          {candidate.local_model}
                        </span>
                        <span className="text-muted-foreground/60 text-xs shrink-0 select-none">
                          →
                        </span>
                        <code
                          className="rounded-md border border-border/60 bg-muted/80 px-2 py-0.5 font-mono text-xs text-foreground break-all shadow-2xs"
                          title={candidate.upstream_model}
                        >
                          {candidate.upstream_model}
                        </code>
                      </div>

                      {/* 右上角操作区：权重输入组件 + 移除按钮 */}
                      <div className="flex items-center gap-2 shrink-0">
                        <div className="flex items-center gap-1.5 bg-muted/50 border border-border/60 rounded-lg px-2 py-1 shadow-2xs">
                          <span className="text-[11px] font-medium text-muted-foreground select-none">
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
                    </div>

                    {/* 次行：属性与状态栏（服务商、模板、协议、可用性健康状态、异常原因） */}
                    <div className="flex flex-wrap items-center gap-2 text-xs">
                      {/* 来源服务商与模板徽标 */}
                      <span className="inline-flex items-center gap-1.5 rounded-md border border-border/50 bg-muted/60 px-2 py-0.5 text-[11px] text-muted-foreground">
                        <Server className="h-3 w-3 text-muted-foreground/80 shrink-0" />
                        <span
                          className="font-medium text-foreground/90 break-words"
                          title={providerName}
                        >
                          {providerName}
                        </span>
                        {candidateTemplateName ? (
                          <span
                            data-testid={`model-pool-selected-template-badge-${candidate.provider_id}`}
                            className="inline-flex items-center gap-0.5 rounded border border-indigo-500/25 bg-indigo-500/10 px-1.5 py-0.2 text-[10px] font-medium text-indigo-700 dark:text-indigo-300 shrink-0"
                            title={candidateTemplateName}
                          >
                            <Sparkles className="h-2.5 w-2.5" />
                            <span>{candidateTemplateName}</span>
                          </span>
                        ) : null}
                      </span>

                      {/* 协议徽标 */}
                      <span
                        className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${protocolBadgeClass(
                          candidate.protocol,
                        )}`}
                      >
                        {protocolShortLabel(candidate.protocol)}
                      </span>

                      {/* 可用性状态徽标 */}
                      <span
                        className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${
                          state.ready
                            ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20"
                            : "bg-amber-500/15 text-amber-700 dark:text-amber-400 border border-amber-500/25"
                        }`}
                      >
                        {label}
                      </span>

                      {/* 异常原因（如果存在） */}
                      {state.reason ? (
                        <span
                          className="inline-flex items-center rounded-md border border-amber-500/20 bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-700 dark:text-amber-400 break-words"
                          title={state.reason}
                        >
                          {state.reason}
                        </span>
                      ) : null}
                    </div>

                    {/* 底部内嵌推理策略面板 */}
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
