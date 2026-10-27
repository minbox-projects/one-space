import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  draftToPriceRow,
  normalizeReasoningEfforts,
  parseContextWindowInput,
  priceRowToDraft,
  providerKeyPool,
  resolveProviderPriceRow,
  resolveTemplateContextWindow,
  type GatewayModelMapping,
  type GatewayPriceDraft,
  type GatewayProviderKey,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
  type GatewayUpstreamProviderWithKeys,
  type ModelPrice,
} from "@/lib/aiGateway";
import { newProviderKeyDraft, runtimeMappingKey } from "../helpers";
import type {
  MappingDraft,
  ProviderDetailDialogProps,
  ProviderKeyDraft,
} from "../types";

export function useProviderDetailForm({
  open,
  onOpenChange,
  provider,
  runtimeProvider,
  prices,
  busy,
  onSave,
  onDelete,
  templates,
  availableTags,
  onDeleteModel,
  onRestoreModel,
  onReenableModel,
  onReenableModels,
  onReenableKey,
}: ProviderDetailDialogProps) {
  const { t } = useTranslation();

  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [keyDrafts, setKeyDrafts] = useState<ProviderKeyDraft[]>([]);
  const [keyModalOpen, setKeyModalOpen] = useState(false);
  const [editingKeyIndex, setEditingKeyIndex] = useState<number | null>(null);
  const [copiedKeyId, setCopiedKeyId] = useState<string | null>(null);
  const [keyNameError, setKeyNameError] = useState(false);
  const [keyValueError, setKeyValueError] = useState(false);
  const [contextError, setContextError] = useState<string | null>(null);
  const [defaultModel, setDefaultModel] = useState("");
  const [protocol, setProtocol] = useState<GatewayUpstreamProtocol>("chat_completions");
  const [mappings, setMappings] = useState<MappingDraft[]>([]);
  const [priceDrafts, setPriceDrafts] = useState<GatewayPriceDraft[]>([]);
  const [autoAddedModel, setAutoAddedModel] = useState<string | null>(null);
  const [expandedMappings, setExpandedMappings] = useState<Record<number, boolean>>(
    {},
  );
  const [effortInputs, setEffortInputs] = useState<Record<number, string>>({});
  const [weight, setWeight] = useState<number | string>(provider?.weight ?? 1);
  const [icon, setIcon] = useState<string>("");
  const [templateId, setTemplateId] = useState<string>("");
  const [tags, setTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState<string>("");
  const [confirmDeleting, setConfirmDeleting] = useState(false);
  const [revealedKeyIds, setRevealedKeyIds] = useState<Record<string, boolean>>({});
  const [expandedReasonKeyIds, setExpandedReasonKeyIds] = useState<Record<string, boolean>>({});

  useEffect(() => {
    setConfirmDeleting(false);
    if (!provider) {
      setName("");
      setBaseUrl("");
      setKeyDrafts([]);
      setKeyNameError(false);
      setKeyValueError(false);
      setContextError(null);
      setDefaultModel("");
      setProtocol("chat_completions");
      setMappings([]);
      setPriceDrafts([]);
      setAutoAddedModel(null);
      setExpandedMappings({});
      setEffortInputs({});
      setWeight(1);
      setIcon("");
      setTemplateId("");
      setTags([]);
      setTagInput("");
      setRevealedKeyIds({});
      setExpandedReasonKeyIds({});
      return;
    }
    const providerPrices = prices ?? [];
    const baseMappings = provider.mappings ?? [];
    const trimmedDefault = (provider.default_model ?? "").trim();
    let nextMappings = baseMappings;
    let nextAutoAdded: string | null = null;
    if (
      trimmedDefault !== "" &&
      !baseMappings.some(
        (mapping) => mapping.upstream_model.trim() === trimmedDefault,
      )
    ) {
      nextMappings = [
        ...baseMappings,
        {
          local_model: trimmedDefault,
          upstream_model: trimmedDefault,
          enabled: true,
        },
      ];
      nextAutoAdded = trimmedDefault;
    }

    const seeded: GatewayPriceDraft[] = [];
    const seenModels = new Set<string>();
    for (const mapping of nextMappings) {
      const model = mapping.upstream_model.trim();
      if (model === "" || seenModels.has(model)) continue;
      seenModels.add(model);
      seeded.push(
        priceRowToDraft(
          resolveProviderPriceRow(providerPrices, provider.id, model),
          model,
          `price-${model}`,
        ),
      );
    }

    setName(provider.name);
    setBaseUrl(provider.base_url);
    setKeyDrafts(
      providerKeyPool(provider).map((key) => ({ ...key, isNew: false })),
    );
    setKeyNameError(false);
    setKeyValueError(false);
    setContextError(null);
    setDefaultModel(provider.default_model ?? "");
    setProtocol(provider.protocol ?? "chat_completions");
    const boundTemplate = provider.template_id
      ? templates?.find((view) => view.template.id === provider.template_id)
          ?.template
      : undefined;
    setMappings(
      nextMappings.map((mapping) => {
        const templateContext = boundTemplate
          ? resolveTemplateContextWindow(boundTemplate, mapping.upstream_model)
          : null;
        const initialContext = mapping.context_window ?? templateContext;
        return {
          ...mapping,
          context_window: initialContext,
          contextDraft:
            initialContext != null ? String(initialContext) : "",
        };
      }),
    );
    setPriceDrafts(seeded);
    setAutoAddedModel(nextAutoAdded);
    setExpandedMappings({});
    setEffortInputs({});
    setWeight(provider.weight ?? 1);
    setIcon(provider.icon ?? "");
    setTemplateId(provider.template_id ?? "");
    setTags(provider.tags ?? []);
    setTagInput("");
    setRevealedKeyIds({});
    setExpandedReasonKeyIds({});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [provider, open]);

  const candidateTags = [
    ...new Set([...(availableTags ?? []), "LLM", "Embedding", "Vision", "Code", "Agent", "Reasoning"]),
  ].filter(Boolean);

  const handleAddTag = (rawTag: string) => {
    const trimmed = rawTag.trim();
    if (!trimmed) return;
    if (!tags.includes(trimmed)) {
      setTags((prev) => [...prev, trimmed]);
    }
    setTagInput("");
  };

  const handleRemoveTag = (tagToRemove: string) => {
    setTags((prev) => prev.filter((t) => t !== tagToRemove));
  };

  const handleToggleTag = (tag: string) => {
    if (tags.includes(tag)) {
      handleRemoveTag(tag);
    } else {
      handleAddTag(tag);
    }
  };

  const handleTagInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleAddTag(tagInput);
    } else if (e.key === "Backspace" && !tagInput && tags.length > 0) {
      handleRemoveTag(tags[tags.length - 1]);
    }
  };

  // 实时运行时合并：在弹窗打开时，把 `runtimeProvider` 快照中匹配行的五个运行时
  // 字段并入本地 mappings 草稿，绝不重置草稿或覆盖用户可编辑字段。effect 幂等：
  // 逐字段比较，无变化时返回原数组（不触发重渲染），因此可在每次渲染后安全运行
  // 而不循环，也能捕获运行时快照被原地更新（引用不变）的情况。
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!open || !runtimeProvider) return;
    const runtimeByKey = new Map<string, GatewayModelMapping>();
    for (const mapping of runtimeProvider.mappings ?? []) {
      runtimeByKey.set(
        runtimeMappingKey(mapping.local_model, mapping.upstream_model),
        mapping,
      );
    }
    setMappings((prev) => {
      let changed = false;
      const next = prev.map((mapping) => {
        const runtime = runtimeByKey.get(
          runtimeMappingKey(mapping.local_model, mapping.upstream_model),
        );
        if (!runtime) return mapping;
        if (
          mapping.auto_disabled === runtime.auto_disabled &&
          mapping.disabled_reason === runtime.disabled_reason &&
          mapping.disabled_at === runtime.disabled_at &&
          mapping.consecutive_failures === runtime.consecutive_failures &&
          mapping.last_error_at === runtime.last_error_at
        ) {
          return mapping;
        }
        changed = true;
        return {
          ...mapping,
          auto_disabled: runtime.auto_disabled,
          disabled_reason: runtime.disabled_reason,
          disabled_at: runtime.disabled_at,
          consecutive_failures: runtime.consecutive_failures,
          last_error_at: runtime.last_error_at,
        };
      });
      return changed ? next : prev;
    });
  });

  // 密钥运行时合并：与映射行同一约定，只把 `runtimeProvider` 中同 id 密钥的运行时
  // 字段并入草稿，保留用户未保存的名称、值与启用开关。幂等，故可每次渲染后运行，
  // 也能捕获手动重新启用后快照被原地清空（引用不变）的情况。
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!open || !runtimeProvider) return;
    const runtimeById = new Map<string, GatewayProviderKey>();
    for (const key of providerKeyPool(runtimeProvider)) {
      runtimeById.set(key.id, key);
    }
    setKeyDrafts((prev) => {
      let changed = false;
      const next = prev.map((draft) => {
        const runtime = runtimeById.get(draft.id);
        if (!runtime) return draft;
        if (
          draft.auto_marked === runtime.auto_marked &&
          draft.failure_kind === runtime.failure_kind &&
          draft.marked_at === runtime.marked_at &&
          draft.reason === runtime.reason
        ) {
          return draft;
        }
        changed = true;
        return {
          ...draft,
          auto_marked: runtime.auto_marked,
          failure_kind: runtime.failure_kind,
          marked_at: runtime.marked_at,
          reason: runtime.reason,
        };
      });
      return changed ? next : prev;
    });
  });

  const draftForModel = (model: string): GatewayPriceDraft =>
    priceDrafts.find((draft) => draft.upstream_model === model) ??
    priceRowToDraft(undefined, model, `price-${model}`);

  const updateDraftForModel = (
    model: string,
    patch: Partial<GatewayPriceDraft>,
  ) => {
    setPriceDrafts((prev) => {
      if (!prev.some((draft) => draft.upstream_model === model)) {
        return [
          ...prev,
          { ...priceRowToDraft(undefined, model, `price-${model}`), ...patch },
        ];
      }
      return prev.map((draft) =>
        draft.upstream_model === model ? { ...draft, ...patch } : draft,
      );
    });
  };

  const updateMapping = (index: number, patch: Partial<MappingDraft>) => {
    setMappings((prev) =>
      prev.map((entry, entryIndex) =>
        entryIndex === index ? { ...entry, ...patch } : entry,
      ),
    );
  };

  const addEffort = (index: number) => {
    const raw = effortInputs[index] ?? "";
    setMappings((prev) =>
      prev.map((entry, entryIndex) =>
        entryIndex === index
          ? {
              ...entry,
              reasoning_efforts: normalizeReasoningEfforts([
                ...(entry.reasoning_efforts ?? []),
                raw,
              ]),
            }
          : entry,
      ),
    );
    setEffortInputs((prev) => ({ ...prev, [index]: "" }));
  };

  const removeEffort = (index: number, effort: string) => {
    setMappings((prev) =>
      prev.map((entry, entryIndex) =>
        entryIndex === index
          ? {
              ...entry,
              reasoning_efforts: normalizeReasoningEfforts(
                (entry.reasoning_efforts ?? []).filter((item) => item !== effort),
              ),
            }
          : entry,
      ),
    );
  };

  // Handlers run only while the dialog has a provider; the facade returns null otherwise.
  const activeProvider = provider as GatewayUpstreamProvider;
  const isTemplateBound = Boolean(provider?.template_id);

  const handleRemoveMapping = (index: number) => {
    const mapping = mappings[index];
    if (
      isTemplateBound &&
      onDeleteModel &&
      mapping?.upstream_model.trim()
    ) {
      onDeleteModel(activeProvider.id, mapping.upstream_model);
    }
    const removedModel = mapping?.upstream_model.trim() ?? "";
    const nextMappings = mappings.filter((_, entryIndex) => entryIndex !== index);
    setMappings(nextMappings);
    if (
      removedModel !== "" &&
      removedModel === defaultModel.trim() &&
      !nextMappings.some((entry) => entry.upstream_model.trim() === removedModel)
    ) {
      setDefaultModel("");
    }
    if (autoAddedModel !== null && removedModel === autoAddedModel) {
      setAutoAddedModel(null);
    }
  };

  const handleAddKey = () => {
    setKeyNameError(false);
    setKeyValueError(false);
    setEditingKeyIndex(null);
    setKeyModalOpen(true);
  };

  const handleEditKey = (index: number) => {
    setKeyNameError(false);
    setKeyValueError(false);
    setEditingKeyIndex(index);
    setKeyModalOpen(true);
  };

  const handleKeyModalConfirm = (
    confirmedName: string,
    confirmedValue: string,
    confirmedDailyReset: boolean,
  ) => {
    setKeyNameError(false);
    setKeyValueError(false);
    if (editingKeyIndex === null) {
      const newKey: ProviderKeyDraft = {
        ...newProviderKeyDraft(),
        name: confirmedName,
        value: confirmedValue,
        daily_reset: confirmedDailyReset,
      };
      setKeyDrafts((prev) => [...prev, newKey]);
    } else {
      setKeyDrafts((prev) =>
        prev.map((entry, entryIndex) =>
          entryIndex === editingKeyIndex
            ? {
                ...entry,
                name: confirmedName,
                value: confirmedValue,
                daily_reset: confirmedDailyReset,
              }
            : entry,
        ),
      );
    }
  };

  const removeKey = (index: number) => {
    setKeyDrafts((prev) => prev.filter((_, entryIndex) => entryIndex !== index));
    setKeyNameError(false);
    setKeyValueError(false);
  };

  const handleKeyModalDelete = () => {
    if (editingKeyIndex !== null) {
      removeKey(editingKeyIndex);
      setEditingKeyIndex(null);
    }
  };

  const updateKey = (index: number, patch: Partial<ProviderKeyDraft>) => {
    if (patch.name !== undefined) setKeyNameError(false);
    if (patch.value !== undefined) setKeyValueError(false);
    setKeyDrafts((prev) =>
      prev.map((entry, entryIndex) =>
        entryIndex === index ? { ...entry, ...patch } : entry,
      ),
    );
  };

  const toggleKeyVisibility = (keyId: string) => {
    setRevealedKeyIds((prev) => ({
      ...prev,
      [keyId]: !prev[keyId],
    }));
  };

  const toggleReasonExpansion = (keyId: string) => {
    setExpandedReasonKeyIds((prev) => ({
      ...prev,
      [keyId]: !prev[keyId],
    }));
  };

  const handleCopyKey = async (keyId: string, keyValue: string) => {
    if (!keyValue) return;
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(keyValue);
        setCopiedKeyId(keyId);
        setTimeout(() => {
          setCopiedKeyId((current) => (current === keyId ? null : current));
        }, 1500);
      }
    } catch {
      // ignore
    }
  };

  const handleSave = async () => {
    // A name is always required; a brand-new key additionally needs a value,
    // while a blank edit of an existing key keeps the stored value backend-side.
    if (keyDrafts.some((key) => key.name.trim() === "")) {
      setKeyNameError(true);
      setKeyValueError(false);
      return;
    }
    if (keyDrafts.some((key) => key.isNew && key.value.trim() === "")) {
      setKeyValueError(true);
      setKeyNameError(false);
      return;
    }
    setKeyNameError(false);
    setKeyValueError(false);
    setContextError(null);
    const savedKeys: GatewayProviderKey[] = keyDrafts.map((key) => ({
      id: key.id,
      name: key.name.trim(),
      value: key.value,
      enabled: key.enabled,
      daily_reset: key.daily_reset === true,
      auto_marked: key.auto_marked,
      failure_kind: key.failure_kind,
      marked_at: key.marked_at,
      reason: key.reason,
    }));
    let invalidContextModel: string | null = null;
    const savedMappings = mappings.map((mapping) => {
      const reasoningEfforts = normalizeReasoningEfforts(mapping.reasoning_efforts);
      const { contextDraft, ...mappingFields } = mapping;
      const rawDraft =
        contextDraft ??
        (mapping.context_window != null ? String(mapping.context_window) : "");
      const parsed = parseContextWindowInput(rawDraft);
      if (parsed.invalid) invalidContextModel = mapping.upstream_model.trim();
      const contextWindow = parsed.value;
      return {
        ...mappingFields,
        enabled: mapping.enabled !== false,
        display_name: mapping.display_name?.trim()
          ? mapping.display_name.trim()
          : undefined,
        protocol: mapping.protocol ? mapping.protocol : undefined,
        reasoning_efforts: reasoningEfforts.length > 0 ? reasoningEfforts : undefined,
        context_window: contextWindow,
      };
    });
    if (invalidContextModel) {
      setContextError(
        t("aiGatewayContextWindowInvalid", {
          model: invalidContextModel,
          defaultValue:
            "Context window must be a whole number between 1 and 4294967295.",
        }),
      );
      return;
    }
    const savedModels = new Set(
      savedMappings
        .map((mapping) => mapping.upstream_model.trim())
        .filter((model) => model !== ""),
    );
    const submittedPrices: ModelPrice[] = [];
    for (const draft of priceDrafts) {
      if (!savedModels.has(draft.upstream_model.trim())) continue;
      const row = draftToPriceRow(draft);
      if (!row) continue;
      submittedPrices.push(activeProvider.id ? { ...row, provider_id: activeProvider.id } : row);
    }
    const draft: GatewayUpstreamProviderWithKeys = {
      ...activeProvider,
      name: name.trim(),
      base_url: baseUrl.trim(),
      keys: savedKeys,
      default_model: defaultModel.trim() ? defaultModel.trim() : null,
      protocol,
      template_id: templateId.trim() ? templateId.trim() : null,
      mappings: savedMappings,
      weight:
        Number.isInteger(Number(weight)) &&
        Number(weight) >= 1 &&
        Number(weight) <= 100
          ? Number(weight)
          : 1,
      icon: icon.trim() ? icon.trim() : null,
      tags: Array.from(new Set(tags.map((t) => t.trim()).filter(Boolean))),
    };
    // Persist through the existing provider upsert wrapper; only claim success
    // and close once it resolves without an explicit failure. A rejected or
    // `false` save keeps the local draft and the dialog unchanged (AC-025).
    try {
      const result = await onSave(draft, submittedPrices);
      if (result === false) return;
    } catch {
      return;
    }
    onOpenChange(false);
  };

  const handleDelete = () => {
    if (!activeProvider.id || !onDelete) return;
    setConfirmDeleting(true);
  };

  const handleCancelDelete = () => {
    setConfirmDeleting(false);
  };

  const handleConfirmDelete = async () => {
    if (!activeProvider.id || !onDelete) return;
    setConfirmDeleting(false);
    const result = await onDelete(activeProvider.id);
    if (result !== false) {
      onOpenChange(false);
    }
  };
  return {
    open,
    onOpenChange,
    provider: activeProvider,
    runtimeProvider,
    prices,
    busy,
    onSave,
    onDelete,
    templates,
    availableTags,
    onDeleteModel,
    onRestoreModel,
    onReenableModel,
    onReenableModels,
    onReenableKey,
    t,
    name,
    setName,
    baseUrl,
    setBaseUrl,
    keyDrafts,
    setKeyDrafts,
    keyModalOpen,
    setKeyModalOpen,
    editingKeyIndex,
    setEditingKeyIndex,
    copiedKeyId,
    setCopiedKeyId,
    keyNameError,
    setKeyNameError,
    keyValueError,
    setKeyValueError,
    contextError,
    setContextError,
    defaultModel,
    setDefaultModel,
    protocol,
    setProtocol,
    mappings,
    setMappings,
    priceDrafts,
    setPriceDrafts,
    autoAddedModel,
    setAutoAddedModel,
    expandedMappings,
    setExpandedMappings,
    effortInputs,
    setEffortInputs,
    weight,
    setWeight,
    icon,
    setIcon,
    templateId,
    setTemplateId,
    tags,
    setTags,
    tagInput,
    setTagInput,
    confirmDeleting,
    setConfirmDeleting,
    revealedKeyIds,
    setRevealedKeyIds,
    expandedReasonKeyIds,
    setExpandedReasonKeyIds,
    candidateTags,
    handleAddTag,
    handleRemoveTag,
    handleToggleTag,
    handleTagInputKeyDown,
    draftForModel,
    updateDraftForModel,
    updateMapping,
    addEffort,
    removeEffort,
    handleRemoveMapping,
    handleAddKey,
    handleEditKey,
    handleKeyModalConfirm,
    removeKey,
    handleKeyModalDelete,
    updateKey,
    toggleKeyVisibility,
    toggleReasonExpansion,
    handleCopyKey,
    handleSave,
    handleDelete,
    handleCancelDelete,
    handleConfirmDelete,
  };
}

export type ProviderDetailForm = ReturnType<typeof useProviderDetailForm>;
