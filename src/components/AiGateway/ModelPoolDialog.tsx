import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Layers, Trash2 } from "lucide-react";
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
  formatContextWindow,
  formatContextWindowDisplay,
  modelPoolCandidateKey,
  modelPoolCandidateServesEffort,
  newModelPoolId,
  resolveMappingContextWindow,
  resolveModelPoolCandidateState,
  resolveModelPoolDefaultEffort,
  selectPoolReasoningCoverageRow,
  type GatewayModelPoolCandidate,
  type GatewayModelPoolReasoning,
  type GatewayUpstreamProtocol,
} from "@/lib/aiGateway";
import { FallbackModelsSection } from "./modelPool/FallbackModelsSection";
import { SelectedCandidatesSection } from "./modelPool/SelectedCandidatesSection";
import { SourceModelsSection } from "./modelPool/SourceModelsSection";
import {
  deriveReasoning,
  effortDeclarationSignature,
  optionValueToAction,
  parseWeight,
  reasoningDraftFrom,
  toDraft,
  type DraftCandidate,
  type ReasoningDraft,
} from "./modelPool/reasoningDraft";
import type {
  ModelPoolCandidateStatus,
  ModelPoolDialogProps,
} from "./modelPool/types";

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
  onDelete,
}: ModelPoolDialogProps) {
  const { t } = useTranslation();
  const { pushToast } = useToast();

  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    if (!open) {
      setConfirmingDelete(false);
      setDeleting(false);
    }
  }, [open]);

  // Seeded once per mount; the console remounts this dialog per open/target so
  // an in-flight runtime refresh can never overwrite an unsaved draft.
  const [name, setName] = useState(pool ? pool.name : "Free Modle");
  const [model, setModel] = useState(pool ? pool.model : "free-model");
  const [selected, setSelected] = useState<DraftCandidate[]>(() =>
    (pool?.candidates ?? []).map(toDraft),
  );

  // 只能从已选模型的上下文长度中去重选择
  const candidateContextOptions = useMemo(() => {
    const set = new Set<number>();
    for (const candidate of selected) {
      const cw = resolveMappingContextWindow(
        providers,
        candidate.provider_id,
        candidate.local_model,
        candidate.upstream_model,
      );
      if (cw != null && cw > 0) {
        set.add(cw);
      }
    }
    return Array.from(set).sort((a, b) => a - b);
  }, [selected, providers]);

  // 已选模型的最大上下文长度
  const maxCandidateContext = useMemo(() => {
    if (candidateContextOptions.length === 0) return null;
    return Math.max(...candidateContextOptions);
  }, [candidateContextOptions]);

  // 用户手动指定的上下文限制（null 表示使用默认最大值）
  const [manualContextWindow, setManualContextWindow] = useState<number | null>(
    () => pool?.context_window ?? null,
  );

  // 最终生效的上下文限制：优先用户手动指定的有效值，否则默认使用已选模型的最大上下文长度
  const effectiveContextWindow = useMemo(() => {
    if (
      manualContextWindow != null &&
      candidateContextOptions.includes(manualContextWindow)
    ) {
      return manualContextWindow;
    }
    return maxCandidateContext;
  }, [manualContextWindow, candidateContextOptions, maxCandidateContext]);

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
    const map = new Map<string, ModelPoolCandidateStatus>();
    for (const candidate of poolStatus?.candidates ?? []) {
      map.set(modelPoolCandidateKey(candidate), candidate);
    }
    return map;
  }, [poolStatus]);

  const unavailableSelectedKeys = useMemo(
    () =>
      selected
        .filter(
          (candidate) =>
            !resolveModelPoolCandidateState(
              candidate,
              providers,
              statusByKey.get(candidate.key) ?? null,
            ).ready,
        )
        .map((candidate) => candidate.key),
    [providers, selected, statusByKey],
  );
  const unavailableSelectedCount = unavailableSelectedKeys.length;

  const sourceGroups = useMemo(() => {
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

  const addFallbackModel = (fallbackModelId: string) => {
    const trimmed = fallbackModelId.trim();
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

  const removeFallbackModel = (fallbackModelId: string) => {
    setFallbackModels((previous) => previous.filter((item) => item !== fallbackModelId));
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
      fallbackModels.flatMap((fallbackModelId) =>
        expandModelPoolFallbackModel(providers, fallbackModelId),
      ),
    [fallbackModels, providers],
  );

  // Live static coverage derived from the current binding drafts and the
  // current provider list; readiness stays a separate, key/health-aware count.
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

  // 一键平分所有已选模型的权重
  const handleEqualizeWeights = () => {
    if (selected.length === 0) return;
    const nextWeights: Record<string, string> = {};
    for (const item of selected) {
      nextWeights[item.key] = "1";
    }
    setWeights((prev) => ({ ...prev, ...nextWeights }));
    setError(null);
  };

  // 一键移除所有当前不可用的已选候选
  const handleClearUnavailable = () => {
    if (unavailableSelectedKeys.length === 0) return;
    const keysToRemove = new Set(unavailableSelectedKeys);
    setSelected((previous) =>
      previous.filter((item) => !keysToRemove.has(item.key)),
    );
    setError(null);
  };

  // 计算权重总和用于显示占比百分比
  const totalWeight = useMemo(() => {
    let sum = 0;
    for (const item of selected) {
      const w = parseWeight(weights[item.key] ?? "1");
      if (w !== null) {
        sum += w;
      }
    }
    return sum;
  }, [selected, weights]);

  const getWeightPercentage = (key: string): string => {
    if (totalWeight <= 0) return "--";
    const w = parseWeight(weights[key] ?? "1");
    if (w === null) return "--";
    return `${Math.round((w / totalWeight) * 100)}%`;
  };

  const handleToggleCollapsed = (providerId: string) => {
    setCollapsed((previous) => ({
      ...previous,
      [providerId]: !previous[providerId],
    }));
  };

  const handleWeightChange = (key: string, value: string) => {
    setWeights((previous) => ({
      ...previous,
      [key]: value,
    }));
  };

  const handleToggleReasoningExpanded = (key: string) => {
    setExpandedReasoning((previous) => ({
      ...previous,
      [key]: !previous[key],
    }));
  };

  const handleDefaultEffortChange = (value: string) => {
    setDefaultEffort(value);
    setError(null);
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
        ...(pool?.fallback_models ?? []).flatMap((fallbackModelId) =>
          expandModelPoolFallbackModel(providers, fallbackModelId),
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

    const contextWindow: number | null =
      candidateContextOptions.length > 0 ? effectiveContextWindow : null;

    setError(null);
    setSaving(true);
    try {
      await onSave({
        id: pool?.id || newModelPoolId(),
        name: trimmedName,
        model: trimmedModel,
        enabled: pool?.enabled ?? true,
        default_reasoning_effort: defaultEffort,
        context_window: contextWindow,
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

  const handleDelete = async () => {
    if (!pool || !onDelete || busy || saving || deleting) return;
    setDeleting(true);
    try {
      const result = await onDelete(pool.id);
      if (result !== false) {
        onOpenChange(false);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : errorToMessage(err));
    } finally {
      setDeleting(false);
      setConfirmingDelete(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        data-testid="model-pool-dialog"
        className="w-full sm:max-w-4xl lg:max-w-5xl max-h-[90vh] overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0 shadow-2xl"
      >
        <DialogHeader className="pl-6 pr-14 py-4 border-b bg-card/80 backdrop-blur-sm shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-2xs">
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

        <div className="flex-1 overflow-y-auto space-y-4 p-5 sm:p-6">
          {/* Section 1: 基础配置与统一上下文限制 */}
          <div className="rounded-2xl border border-border/70 bg-card/60 p-4 shadow-2xs space-y-3.5">
            <div className="grid gap-3.5 sm:grid-cols-2 lg:grid-cols-3">
              {/* Pool Name */}
              <label className="flex flex-col gap-1.5 text-xs font-medium text-foreground">
                <span className="flex items-center gap-1">
                  <span>{t("aiGatewayModelPoolNameLabel", "Name")}</span>
                  <span className="text-destructive">*</span>
                </span>
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

              {/* Public Model ID */}
              <label className="flex flex-col gap-1.5 text-xs font-medium text-foreground">
                <span className="flex items-center gap-1">
                  <span>{t("aiGatewayModelPoolModelLabel", "Public model ID")}</span>
                  <span className="text-destructive">*</span>
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

              {/* Unified Context Window Limit */}
              <div
                data-testid="model-pool-context-limit-container"
                className="flex flex-col gap-1.5 text-xs font-medium text-foreground sm:col-span-2 lg:col-span-1"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="flex items-center gap-1.5 truncate">
                    <Layers className="h-3.5 w-3.5 text-cyan-600 dark:text-cyan-400 shrink-0" />
                    <span className="truncate">
                      {t(
                        "aiGatewayModelPoolContextLimitTitle",
                        "Unified Context Window Limit",
                      )}
                    </span>
                  </span>
                  {effectiveContextWindow != null && (
                    <span
                      data-testid="model-pool-context-approx"
                      className="font-mono text-[11px] text-cyan-600 dark:text-cyan-400 font-semibold shrink-0"
                    >
                      {t("aiGatewayContextWindowApproxTokens", {
                        tokens: formatContextWindow(effectiveContextWindow),
                        defaultValue: `≈ ${formatContextWindow(effectiveContextWindow)} tokens`,
                      })}
                    </span>
                  )}
                </div>
                <select
                  data-testid="model-pool-context-select"
                  aria-label={t(
                    "aiGatewayModelPoolContextLimitTitle",
                    "Unified Context Window Limit",
                  )}
                  value={effectiveContextWindow ?? ""}
                  onChange={(event) => {
                    const val = event.target.value;
                    setManualContextWindow(val ? Number(val) : null);
                    setError(null);
                  }}
                  disabled={busy || saving || candidateContextOptions.length === 0}
                  className="h-9 w-full rounded-lg border border-border/80 bg-background px-3 font-mono text-xs sm:text-sm text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {candidateContextOptions.length === 0 ? (
                    <option value="">
                      {t(
                        "aiGatewayModelPoolContextLimitNoOptions",
                        "No context limits detected from selected models",
                      )}
                    </option>
                  ) : (
                    candidateContextOptions.map((cw) => {
                      const isMax = cw === maxCandidateContext;
                      const displayLabel = formatContextWindowDisplay(cw);
                      const exactTokens = formatContextWindow(cw);
                      const suffix = isMax
                        ? ` - ${t(
                            "aiGatewayModelPoolContextLimitMaxDefault",
                            "Maximum (default)",
                          )}`
                        : "";
                      return (
                        <option key={cw} value={cw}>
                          {`${displayLabel} (${exactTokens} tokens)${suffix}`}
                        </option>
                      );
                    })
                  )}
                </select>
              </div>
            </div>

            {/* 指标与就绪状态栏 */}
            <div className="flex flex-wrap items-center justify-between gap-3 pt-1 border-t border-border/50 text-[11px] text-muted-foreground">
              <div className="flex flex-wrap items-center gap-3">
                <span className="inline-flex items-center gap-1.5">
                  <span className="h-2 w-2 rounded-full bg-emerald-500" />
                  <span>
                    {t(
                      "aiGatewayModelPoolReasoningRuntimeReadinessLabel",
                      "Runtime readiness",
                    )}
                    :
                  </span>
                  <span
                    data-testid="model-pool-reasoning-runtime-readiness"
                    className="font-medium text-foreground"
                  >
                    {t("aiGatewayModelPoolReasoningRuntimeReadyCount", {
                      ready: readyCount,
                      total: selected.length,
                      defaultValue: "{{ready}}/{{total}} candidates ready",
                    })}
                  </span>
                </span>
                <span className="text-border">|</span>
                <span className="inline-flex items-center gap-1.5">
                  <span>
                    {t("aiGatewayModelPoolReasoningCoverageLabel", "Static coverage")}:
                  </span>
                  <span
                    data-testid="model-pool-reasoning-static-coverage"
                    className="font-medium text-foreground"
                  >
                    {t("aiGatewayModelPoolReasoningCoverageValue", {
                      mapped: coverageRow?.mapped_candidates ?? 0,
                      total: coverageRow?.total_candidates ?? selected.length,
                      defaultValue: "{{mapped}}/{{total}} candidates mapped",
                    })}
                  </span>
                </span>
              </div>
              <div className="text-[10px] text-muted-foreground/80 hidden sm:block">
                {t(
                  "aiGatewayModelPoolContextLimitDesc",
                  "Select the unified public context window for this model pool, used during AI terminal synchronization. Choices are derived from selected models; defaults to the maximum context window.",
                )}
              </div>
            </div>
          </div>

          {/* Section 2: 核心成员穿梭工作区（双栏布局） */}
          <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-start">
            <SourceModelsSection
              t={t}
              sourceGroups={sourceGroups}
              filteredCandidates={filteredCandidates}
              allFilteredSelected={allFilteredSelected}
              normalizedQuery={normalizedQuery}
              query={query}
              onQueryChange={setQuery}
              collapsed={collapsed}
              onToggleCollapsed={handleToggleCollapsed}
              selectedKeys={selectedKeys}
              onToggleCandidate={toggleCandidate}
              onToggleSelectAllFiltered={toggleSelectAllFiltered}
              providers={providers}
              statusByKey={statusByKey}
              busy={busy}
              saving={saving}
            />

            <SelectedCandidatesSection
              t={t}
              selected={selected}
              providers={providers}
              templates={templates}
              statusByKey={statusByKey}
              weights={weights}
              onWeightChange={handleWeightChange}
              getWeightPercentage={getWeightPercentage}
              onRemoveCandidate={removeCandidate}
              unavailableSelectedCount={unavailableSelectedCount}
              onClearUnavailable={handleClearUnavailable}
              onEqualizeWeights={handleEqualizeWeights}
              busy={busy}
              saving={saving}
              reasoningDrafts={reasoningDrafts}
              expandedReasoning={expandedReasoning}
              onToggleReasoningExpanded={handleToggleReasoningExpanded}
              onUpdateReasoning={updateReasoning}
            />
          </div>

          {/* Section 3: 高级路由策略与故障降级 */}
          <FallbackModelsSection
            t={t}
            defaultEffort={defaultEffort}
            onDefaultEffortChange={handleDefaultEffortChange}
            fallbackModels={fallbackModels}
            fallbackQuery={fallbackQuery}
            setFallbackQuery={setFallbackQuery}
            fallbackDropdownOpen={fallbackDropdownOpen}
            setFallbackDropdownOpen={setFallbackDropdownOpen}
            fallbackComboboxRef={fallbackComboboxRef}
            fallbackInputRef={fallbackInputRef}
            fallbackProviderCounts={fallbackProviderCounts}
            fallbackModelOptions={fallbackModelOptions}
            filteredFallbackModelOptions={filteredFallbackModelOptions}
            filteredFallbackModels={filteredFallbackModels}
            onAddFallbackModel={addFallbackModel}
            onMoveFallbackModel={moveFallbackModel}
            onRemoveFallbackModel={removeFallbackModel}
            onClearFallbackModels={clearFallbackModels}
            busy={busy}
            saving={saving}
          />

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

        <DialogFooter className="px-6 py-4 border-t bg-card/80 backdrop-blur-sm shrink-0 flex flex-row items-center justify-between sm:justify-between sm:space-x-0">
          <div className="flex items-center">
            {pool && onDelete ? (
              confirmingDelete ? (
                <div className="flex items-center gap-2">
                  <span className="text-xs font-medium text-destructive">
                    {t("aiGatewayModelPoolDeleteConfirm", "Delete model pool?")}
                  </span>
                  <button
                    type="button"
                    data-testid="ai-gateway-model-pool-delete-confirm"
                    onClick={() => void handleDelete()}
                    disabled={busy || saving || deleting}
                    className="inline-flex h-8 items-center rounded-lg bg-destructive px-2.5 text-xs font-medium text-destructive-foreground transition hover:bg-destructive/90 disabled:opacity-50"
                  >
                    {t("aiGatewayModelPoolDeleteConfirmButton", "Delete")}
                  </button>
                  <button
                    type="button"
                    onClick={() => setConfirmingDelete(false)}
                    disabled={busy || saving || deleting}
                    className="inline-flex h-8 items-center rounded-lg border border-border/80 bg-background px-2.5 text-xs font-medium text-foreground transition hover:bg-muted disabled:opacity-50"
                  >
                    {t("aiGatewayModelPoolDeleteCancel", "Cancel")}
                  </button>
                </div>
              ) : (
                <button
                  type="button"
                  data-testid={`ai-gateway-model-pool-delete-${pool.id}`}
                  onClick={() => setConfirmingDelete(true)}
                  disabled={busy || saving || deleting}
                  className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-destructive/30 bg-background px-3 text-xs font-medium text-destructive transition hover:bg-destructive/10 disabled:opacity-50"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  <span>{t("aiGatewayModelPoolDelete", "Delete")}</span>
                </button>
              )
            ) : null}
          </div>

          <div className="flex items-center gap-2 ml-auto">
            <button
              type="button"
              data-testid="model-pool-cancel"
              onClick={() => onOpenChange(false)}
              disabled={busy || saving || deleting}
              className="inline-flex h-8 items-center rounded-lg border border-border/80 bg-background px-3 text-xs font-medium text-foreground transition hover:bg-muted disabled:opacity-50"
            >
              {t("aiGatewayModelPoolCancel", "Cancel")}
            </button>
            <button
              type="button"
              data-testid="model-pool-save"
              onClick={() => void handleSave()}
              disabled={busy || saving || deleting}
              className="inline-flex h-8 items-center rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-xs transition hover:bg-primary/90 disabled:opacity-50"
            >
              {t("aiGatewayModelPoolSave", "Save")}
            </button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
