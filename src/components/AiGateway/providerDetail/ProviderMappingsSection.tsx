import {
  ArchiveRestore,
  ChevronDown,
  ChevronUp,
  Eye,
  EyeOff,
  Layers,
  Plus,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import {
  CONTEXT_WINDOW_PRESET_ITEMS,
  formatContextWindow,
  formatContextWindowCompact,
  formatContextWindowDisplay,
  isMappingDeprecated,
  parseContextWindowInput,
  resolveTemplateContextWindow,
  type GatewayUpstreamProtocol,
} from "@/lib/aiGateway";
import { MappingPriceEditor } from "../MappingPriceEditor";
import { mappingInputClass } from "./helpers";
import type { ProviderDetailDerived } from "./types";
import type { ProviderDetailForm } from "./hooks/useProviderDetailForm";

export function ProviderMappingsSection({
  form,
  derived,
}: {
  form: ProviderDetailForm;
  derived: ProviderDetailDerived;
}) {
  const {
    t,
    provider,
    busy,
    onReenableModel,
    onReenableModels,
    onRestoreModel,
    mappings,
    setMappings,
    expandedMappings,
    setExpandedMappings,
    effortInputs,
    setEffortInputs,
    autoAddedModel,
    contextError,
    draftForModel,
    updateDraftForModel,
    updateMapping,
    addEffort,
    removeEffort,
    handleRemoveMapping,
  } = form;
  const {
    isTemplateBound,
    boundTemplate,
    ignoredModels,
    autoDisabledModels,
  } = derived;
  return (
    <>
          {/* 模型映射列表（宽幅舒展设计，输入框尺寸与标准 field 保持一致） */}
          <div className="space-y-2 rounded-xl border bg-muted/20 p-3.5">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-xs font-semibold text-foreground">
                    {t("aiGatewayModelMappings", "Model mappings")}
                  </span>
                  <span
                    data-testid="ai-gateway-mappings-count-badge"
                    className="inline-flex items-center rounded-full bg-background px-2 py-0.5 text-[10px] font-semibold text-muted-foreground border border-border/70 shadow-2xs"
                  >
                    {t("aiGatewayConfiguredModelCount", {
                      count: mappings.length,
                      defaultValue: `${mappings.length} configured`,
                    })}
                  </span>
                </div>
                <p className="text-[11px] text-muted-foreground mt-0.5">
                  {t(
                    "aiGatewayModelMappingsDesc",
                    "Map local request model names to upstream models. Optionally set a display name shown in the gateway for each model.",
                  )}
                </p>
              </div>
              <div className="flex items-center gap-2">
                {mappings.length > 0 ? (
                  <>
                    <button
                      type="button"
                      data-testid="ai-gateway-enable-all-mappings"
                      onClick={() =>
                        setMappings((prev) =>
                          prev.map((entry) => ({ ...entry, enabled: true })),
                        )
                      }
                      disabled={
                        busy || mappings.every((entry) => entry.enabled !== false)
                      }
                      aria-label={t("aiGatewayEnableAllMappings", "Enable all")}
                      className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2 text-xs font-medium shadow-sm transition hover:bg-muted disabled:opacity-50"
                    >
                      <Eye className="h-3 w-3" />
                      {t("aiGatewayEnableAllMappings", "Enable all")}
                    </button>
                    <button
                      type="button"
                      data-testid="ai-gateway-disable-all-mappings"
                      onClick={() =>
                        setMappings((prev) =>
                          prev.map((entry) => ({ ...entry, enabled: false })),
                        )
                      }
                      disabled={
                        busy || mappings.every((entry) => entry.enabled === false)
                      }
                      aria-label={t("aiGatewayDisableAllMappings", "Disable all")}
                      className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2 text-xs font-medium shadow-sm transition hover:bg-muted disabled:opacity-50"
                    >
                      <EyeOff className="h-3 w-3" />
                      {t("aiGatewayDisableAllMappings", "Disable all")}
                    </button>
                  </>
                ) : null}
                {autoDisabledModels.length > 0 && provider.id !== "" && onReenableModels ? (
                  <button
                    type="button"
                    data-testid={`ai-gateway-reenable-models-${provider.id}`}
                    onClick={() => onReenableModels(provider.id)}
                    disabled={busy}
                    className="inline-flex h-7 items-center gap-1.5 rounded-md border border-amber-500/40 bg-background px-2 text-xs font-medium text-amber-700 shadow-sm transition hover:bg-amber-500/15 disabled:opacity-50 dark:text-amber-400"
                  >
                    <RotateCcw className="h-3 w-3" />
                    {t("aiGatewayReenableAllMappings", "Re-enable all mappings")}
                  </button>
                ) : null}
                <button
                  type="button"
                  onClick={() =>
                    setMappings((prev) => [
                      ...prev,
                      {
                        local_model: "",
                        upstream_model: "",
                        display_name: "",
                        protocol: null,
                        enabled: true,
                        contextDraft: "",
                      },
                    ])
                  }
                  className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2 text-xs font-medium shadow-sm transition hover:bg-muted"
                >
                  <Plus className="h-3 w-3" />
                  {t("aiGatewayAddMapping", "Add mapping")}
                </button>
              </div>
            </div>

            {contextError ? (
              <p
                role="alert"
                className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive"
              >
                {contextError}
              </p>
            ) : null}

            {mappings.length === 0 ? (
              <p className="rounded-lg border border-dashed bg-background/50 px-3 py-3 text-center text-xs text-muted-foreground">
                {t("aiGatewayNoMappings", "No model mappings configured.")}
              </p>
            ) : (
              <div className="overflow-x-auto">
                <ul className="space-y-2">
                {mappings.map((mapping, index) => {
                  const deprecated = Boolean(
                    boundTemplate && isMappingDeprecated(mapping, boundTemplate),
                  );
                  const isExpanded = expandedMappings[index] === true;
                  const efforts = mapping.reasoning_efforts ?? [];
                  const upstreamModel = mapping.upstream_model.trim();
                  const isAutoAdded =
                    upstreamModel !== "" && upstreamModel === autoAddedModel;
                  const isAutoDisabled = mapping.auto_disabled === true;
                  const isEffectiveEnabled =
                    mapping.enabled !== false && !isAutoDisabled;
                  return (
                  <li
                    key={index}
                    data-disabled={mapping.enabled === false ? "true" : undefined}
                    data-auto-disabled={isAutoDisabled ? "true" : undefined}
                    data-deprecated={deprecated ? "true" : undefined}
                    data-auto-added={isAutoAdded ? "true" : undefined}
                    className={`space-y-2 rounded-lg ${
                      mapping.enabled === false ? "opacity-60" : ""
                    } ${
                      isAutoDisabled
                        ? "border border-amber-500/40 bg-amber-500/5 px-2 py-1.5"
                        : ""
                    }`}
                  >
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        data-testid={`ai-gateway-mapping-expand-${index}`}
                        aria-expanded={isExpanded}
                        aria-label={t("aiGatewayMappingDetails", "Mapping details")}
                        onClick={() =>
                          setExpandedMappings((prev) => ({
                            ...prev,
                            [index]: !prev[index],
                          }))
                        }
                        className="inline-flex h-[38px] w-[30px] shrink-0 items-center justify-center rounded-lg text-muted-foreground transition hover:bg-muted hover:text-foreground"
                      >
                        {isExpanded ? (
                          <ChevronUp className="h-4 w-4" />
                        ) : (
                          <ChevronDown className="h-4 w-4" />
                        )}
                      </button>
                      <Switch
                        aria-label={t("aiGatewayToggleMappingAria", {
                          index: index + 1,
                          defaultValue: `Enable mapping ${index + 1}`,
                        })}
                        checked={isEffectiveEnabled}
                        onCheckedChange={(checked) => {
                          if (checked) {
                            updateMapping(index, {
                              enabled: true,
                              auto_disabled: false,
                            });
                            if (
                              isAutoDisabled &&
                              provider.id !== "" &&
                              onReenableModel
                            ) {
                              onReenableModel(
                                provider.id,
                                mapping.local_model,
                                mapping.upstream_model,
                              );
                            }
                          } else {
                            updateMapping(index, { enabled: false });
                          }
                        }}
                      />
                      <input
                        type="text"
                        value={mapping.local_model}
                        onChange={(event) =>
                          updateMapping(index, { local_model: event.target.value })
                        }
                        placeholder={t("aiGatewayLocalModelPlaceholder", "local model")}
                        aria-label={t("aiGatewayLocalModelAria", {
                          index: index + 1,
                          defaultValue: `Local model ${index + 1}`,
                        })}
                        className={`${mappingInputClass} min-w-[120px] flex-1 font-mono`}
                      />
                      <span aria-hidden="true" className="shrink-0 text-muted-foreground font-semibold text-sm">
                        →
                      </span>
                      <input
                        type="text"
                        value={mapping.upstream_model}
                        onChange={(event) =>
                          updateMapping(index, { upstream_model: event.target.value })
                        }
                        placeholder={t("aiGatewayUpstreamModelPlaceholder", "upstream model")}
                        aria-label={t("aiGatewayUpstreamModelAria", {
                          index: index + 1,
                          defaultValue: `Upstream model ${index + 1}`,
                        })}
                        className={`${mappingInputClass} min-w-[120px] flex-1 font-mono`}
                      />
                      {deprecated ? (
                        <span className="shrink-0 rounded-full border border-amber-500/40 bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400">
                          {t("aiGatewayTemplateDeprecated")}
                        </span>
                      ) : null}
                      <input
                        type="text"
                        value={mapping.display_name ?? ""}
                        onChange={(event) =>
                          updateMapping(index, { display_name: event.target.value })
                        }
                        placeholder={t("aiGatewayLocalModelNamePlaceholder", "display name")}
                        aria-label={t("aiGatewayLocalModelNameAria", {
                          index: index + 1,
                          defaultValue: `Local model name ${index + 1}`,
                        })}
                        className={`${mappingInputClass} min-w-[120px] flex-1`}
                      />
                      <select
                        value={mapping.protocol ?? ""}
                        onChange={(event) =>
                          updateMapping(index, {
                            protocol:
                              event.target.value === ""
                                ? null
                                : (event.target.value as GatewayUpstreamProtocol),
                          })
                        }
                        aria-label={t("aiGatewayMappingProtocolAria", {
                          index: index + 1,
                          defaultValue: `Mapping protocol ${index + 1}`,
                        })}
                        className={`${mappingInputClass} min-w-[180px] shrink-0`}
                      >
                        <option value="">
                          {t("aiGatewayProtocolInherit", "Inherit from provider")}
                        </option>
                        <option value="chat_completions">
                          {t("aiGatewayProtocolChat", "Chat Completions (/chat/completions)")}
                        </option>
                        <option value="responses">
                          {t("aiGatewayProtocolResponses", "Responses (/responses)")}
                        </option>
                        <option value="anthropic_messages">
                          {t(
                            "aiGatewayProtocolAnthropicMessages",
                            "Anthropic Messages (native)",
                          )}
                        </option>
                      </select>
                      {(() => {
                        const parsedVal = parseContextWindowInput(
                          mapping.contextDraft ??
                            (mapping.context_window != null
                              ? String(mapping.context_window)
                              : ""),
                        ).value;
                        if (parsedVal) {
                          return (
                            <span
                              data-testid={`ai-gateway-mapping-context-badge-${index}`}
                              className="inline-flex items-center gap-1 rounded-md border border-cyan-500/25 bg-cyan-500/10 px-2 py-1 font-mono text-[11px] font-medium text-cyan-700 dark:text-cyan-300 shadow-2xs shrink-0"
                              title={t(
                                "aiGatewayModelListEffectiveContext",
                                "Conservative effective context",
                              )}
                            >
                              <Layers className="h-3 w-3 text-cyan-600/80 dark:text-cyan-400/80 shrink-0" />
                              <span>{formatContextWindowDisplay(parsedVal)}</span>
                            </span>
                          );
                        }
                        return null;
                      })()}
                      <button
                        type="button"
                        onClick={() => handleRemoveMapping(index)}
                        aria-label={t("aiGatewayRemoveMappingAria", {
                          index: index + 1,
                          defaultValue: `Remove mapping ${index + 1}`,
                        })}
                        className="inline-flex h-[38px] w-[38px] shrink-0 items-center justify-center rounded-lg text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                      {isAutoDisabled && provider.id !== "" ? (
                        <button
                          type="button"
                          data-testid={`ai-gateway-reenable-mapping-${mapping.local_model}`}
                          onClick={() =>
                            onReenableModel?.(
                              provider.id,
                              mapping.local_model,
                              mapping.upstream_model,
                            )
                          }
                          disabled={busy}
                          className="inline-flex h-[38px] shrink-0 items-center gap-1.5 rounded-lg border border-amber-500/40 bg-background px-2.5 text-xs font-medium text-amber-700 shadow-sm transition hover:bg-amber-500/15 disabled:opacity-50 dark:text-amber-400"
                        >
                          <RotateCcw className="h-3.5 w-3.5" />
                          {t("aiGatewayReenableMapping", "Re-enable mapping")}
                        </button>
                      ) : null}
                    </div>

                    {isAutoAdded ? (
                      <span className="inline-flex w-fit items-center rounded-full border border-primary/40 bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                        {t("aiGatewayDefaultModelAutoAdded")}
                      </span>
                    ) : null}

                    {isExpanded ? (
                      <>
                        {upstreamModel !== "" ? (
                          <MappingPriceEditor
                            index={index}
                            draft={draftForModel(upstreamModel)}
                            onChange={(patch) =>
                              updateDraftForModel(upstreamModel, patch)
                            }
                          />
                        ) : null}

                        {/* 模型上下文长度限制配置 */}
                        <div
                          data-testid={`ai-gateway-mapping-context-${index}`}
                          className="space-y-2 rounded-lg border-t border-border/60 bg-muted/10 px-3 py-2.5"
                        >
                          <div className="flex items-center justify-between">
                            <label
                              htmlFor={`ai-gateway-context-input-${index}`}
                              className="text-[11px] font-semibold text-foreground"
                            >
                              {t("aiGatewayContextWindowSectionTitle", "模型上下文长度限制")}
                            </label>
                            {isTemplateBound ? (
                              <span className="text-[10px] text-muted-foreground">
                                {boundTemplate
                                  ? resolveTemplateContextWindow(
                                      boundTemplate,
                                      mapping.upstream_model,
                                    ) != null
                                    ? t("aiGatewayContextInheritedFrom", {
                                        name: boundTemplate.name,
                                        defaultValue: `Inherited from ${boundTemplate.name}`,
                                      })
                                    : t(
                                        "aiGatewayContextTemplateModelMissing",
                                        "Template model not found; context unspecified",
                                      )
                                  : t(
                                      "aiGatewayContextBoundTemplateMissing",
                                      "Template not found; context unspecified",
                                    )}
                              </span>
                            ) : null}
                          </div>
                          <p className="text-[11px] text-muted-foreground">
                            {t(
                              "aiGatewayContextWindowSectionDesc",
                              "Limit the maximum context window (tokens) for this model. Supports raw tokens only; leave blank for unlimited.",
                            )}
                          </p>

                          <div className="flex flex-wrap items-center gap-2">
                            <input
                              id={`ai-gateway-context-input-${index}`}
                              type="text"
                              inputMode="numeric"
                              value={
                                mapping.contextDraft ??
                                (mapping.context_window != null
                                  ? String(mapping.context_window)
                                  : "")
                              }
                              onChange={(event) =>
                                updateMapping(index, {
                                  contextDraft: event.target.value,
                                })
                              }
                              placeholder={t(
                                "aiGatewayContextWindowPlaceholder",
                                "例如：128000, 1048576（留空不限制）",
                              )}
                              aria-label={t("aiGatewayContextWindowLabel", {
                                index: index + 1,
                                defaultValue: `Context window (tokens) ${index + 1}`,
                              })}
                              className={`${mappingInputClass} w-48 font-mono`}
                            />

                            {(() => {
                              const currentVal = parseContextWindowInput(
                                mapping.contextDraft ??
                                  (mapping.context_window != null
                                    ? String(mapping.context_window)
                                    : ""),
                              ).value;
                              if (currentVal != null) {
                                return (
                                  <span className="text-xs text-muted-foreground font-mono flex items-center gap-1">
                                    <span>≈</span>
                                    <span className="font-semibold text-foreground/80">
                                      {formatContextWindowCompact(currentVal)}
                                    </span>
                                    <span>({formatContextWindow(currentVal)} tokens)</span>
                                  </span>
                                );
                              }
                              return null;
                            })()}
                          </div>

                          <div className="flex flex-wrap items-center gap-1.5 pt-1">
                            <span className="text-[11px] text-muted-foreground mr-1">
                              {t("aiGatewayContextWindowQuickPreset", "常用预设")}:
                            </span>
                            {CONTEXT_WINDOW_PRESET_ITEMS.map((preset) => {
                              const currentDraft = (
                                mapping.contextDraft ??
                                (mapping.context_window != null
                                  ? String(mapping.context_window)
                                  : "")
                              ).trim();
                              const isSelected =
                                currentDraft === String(preset.tokens);
                              return (
                                <button
                                  key={preset.label}
                                  type="button"
                                  onClick={() =>
                                    updateMapping(index, {
                                      contextDraft: String(preset.tokens),
                                    })
                                  }
                                  className={`inline-flex items-center rounded-md px-2 py-0.5 text-xs font-mono transition shadow-xs ${
                                    isSelected
                                      ? "bg-primary text-primary-foreground font-semibold"
                                      : "border border-border bg-background hover:bg-muted text-muted-foreground hover:text-foreground"
                                  }`}
                                >
                                  {preset.label}
                                </button>
                              );
                            })}
                            {mapping.contextDraft || mapping.context_window != null ? (
                              <button
                                type="button"
                                onClick={() =>
                                  updateMapping(index, {
                                    contextDraft: "",
                                    context_window: null,
                                  })
                                }
                                className="inline-flex items-center rounded-md px-1.5 py-0.5 text-[11px] text-muted-foreground hover:text-destructive transition"
                              >
                                {t("clear", "清除")}
                              </button>
                            ) : null}
                          </div>
                        </div>
                      <div
                        data-testid={`ai-gateway-mapping-efforts-${index}`}
                        className="space-y-2 rounded-lg border-t border-border/60 bg-muted/10 px-3 py-2.5"
                      >
                        <div className="text-[11px] font-semibold text-foreground">
                          {t("aiGatewayReasoningEfforts", "Reasoning efforts")}
                        </div>
                        <p className="text-[11px] text-muted-foreground">
                          {t(
                            "aiGatewayReasoningEffortsDesc",
                            "Add or remove the reasoning-effort identifiers this model advertises.",
                          )}
                        </p>
                        {efforts.length > 0 ? (
                          <div className="flex flex-wrap items-center gap-1.5">
                            {efforts.map((effort) => (
                              <span
                                key={effort}
                                data-testid={`ai-gateway-mapping-effort-${index}-${effort}`}
                                className="inline-flex items-center gap-1 rounded-full border border-border bg-secondary px-2 py-0.5 text-[11px] font-medium text-secondary-foreground"
                              >
                                <span className="font-mono">{effort}</span>
                                <button
                                  type="button"
                                  data-testid={`ai-gateway-mapping-effort-remove-${index}-${effort}`}
                                  aria-label={t("aiGatewayReasoningEffortRemove", {
                                    effort,
                                    defaultValue: `Remove ${effort}`,
                                  })}
                                  onClick={() => removeEffort(index, effort)}
                                  className="inline-flex h-4 w-4 items-center justify-center rounded-full text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                                >
                                  <X className="h-3 w-3" />
                                </button>
                              </span>
                            ))}
                          </div>
                        ) : null}
                        <div className="flex items-center gap-2">
                          <input
                            type="text"
                            data-testid={`ai-gateway-mapping-effort-input-${index}`}
                            value={effortInputs[index] ?? ""}
                            onChange={(event) =>
                              setEffortInputs((prev) => ({
                                ...prev,
                                [index]: event.target.value,
                              }))
                            }
                            onKeyDown={(event) => {
                              if (event.key === "Enter") {
                                event.preventDefault();
                                addEffort(index);
                              }
                            }}
                            placeholder={t(
                              "aiGatewayReasoningEffortPlaceholder",
                              "e.g. high",
                            )}
                            aria-label={t("aiGatewayReasoningEfforts", "Reasoning efforts")}
                            className={`${mappingInputClass} h-8 min-w-[140px] flex-1 font-mono`}
                          />
                          <button
                            type="button"
                            data-testid={`ai-gateway-mapping-effort-add-${index}`}
                            onClick={() => addEffort(index)}
                            className="inline-flex h-8 items-center gap-1.5 rounded-md border bg-background px-2.5 text-xs font-medium shadow-sm transition hover:bg-muted"
                          >
                            <Plus className="h-3 w-3" />
                            {t("aiGatewayReasoningEffortAdd", "Add")}
                          </button>
                        </div>
                      </div>
                      </>
                    ) : null}
                  </li>
                  );
                })}
                </ul>
              </div>
            )}

            {ignoredModels.length > 0 ? (
              <div
                data-testid="ai-gateway-ignored-models"
                className="rounded-lg border border-border/70 bg-background/60 p-2.5"
              >
                <div className="flex items-center gap-1.5">
                  <ArchiveRestore className="h-3.5 w-3.5 text-muted-foreground" />
                  <span className="text-[11px] font-semibold text-foreground">
                    {t("aiGatewayIgnoredModels", "Ignored models")}
                  </span>
                </div>
                <p className="mt-0.5 text-[11px] text-muted-foreground">
                  {t(
                    "aiGatewayIgnoredModelsDesc",
                    "Models removed from this provider, either by you or by a template sync. Restore one to rebuild it from the template's current data.",
                  )}
                </p>
                <ul className="mt-1.5 space-y-1">
                  {ignoredModels.map((model) => (
                    <li
                      key={model}
                      data-testid={`ai-gateway-ignored-model-${model}`}
                      className="flex items-center justify-between gap-2 rounded-md bg-muted/40 px-2 py-1"
                    >
                      <span className="truncate font-mono text-[11px] text-foreground">
                        {model}
                      </span>
                      <button
                        type="button"
                        data-testid={`ai-gateway-restore-model-${model}`}
                        onClick={() => onRestoreModel?.(provider.id, model)}
                        className="inline-flex h-6 shrink-0 items-center gap-1 rounded-md border bg-background px-2 text-[11px] font-medium text-muted-foreground transition hover:bg-muted hover:text-foreground"
                      >
                        <RotateCcw className="h-3 w-3" />
                        {t("aiGatewayRestoreModel", "Restore")}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
          </div>
    </>
  );
}
