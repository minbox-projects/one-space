import { AlertTriangle, X } from "lucide-react";
import type { TFunction } from "i18next";
import {
  SUPPORTED_ROLES,
  SUPPORTED_TOOLS,
  getModelCapability,
  isKnownEffort,
  resolveExcludedEfforts,
  resolveWritableEfforts,
  type AgentMatrixRow,
  type ModelSourcesResult,
  type SupportedTool,
  type ValidEffort,
} from "@/lib/aiWorkflowProfiles";
import { SearchableModelCombobox } from "./SearchableModelCombobox";
import { ROLE_BADGE_STYLES, getRoleStyle } from "./styles";

type ModelMatrixProps = {
  t: TFunction;
  matrix: AgentMatrixRow[];
  originalMatrix: AgentMatrixRow[];
  modelSources: ModelSourcesResult | null;
  isLoading: boolean;
  batchColumnTool: SupportedTool | null;
  setBatchColumnTool: (tool: SupportedTool | null) => void;
  batchColumnValue: string;
  setBatchColumnValue: (value: string) => void;
  handleApplyColumnBatch: (tool: SupportedTool) => void;
  batchRowRole: string | null;
  setBatchRowRole: (role: string | null) => void;
  batchRowValue: string;
  setBatchRowValue: (value: string) => void;
  handleApplyRowBatch: (role: string) => void;
  allCandidateModels: string[];
  editingCell: { role: string; tool: SupportedTool } | null;
  setEditingCell: (cell: { role: string; tool: SupportedTool } | null) => void;
  cellSearchFilter: string;
  setCellSearchFilter: (value: string) => void;
  handleModelChange: (
    role: string,
    tool: SupportedTool,
    model: string,
  ) => void;
  handleEffortChange: (
    role: string,
    tool: SupportedTool,
    reasoningEffort: string,
  ) => void;
};

export function ModelMatrix({
  t,
  matrix,
  originalMatrix,
  modelSources,
  isLoading,
  batchColumnTool,
  setBatchColumnTool,
  batchColumnValue,
  setBatchColumnValue,
  handleApplyColumnBatch,
  batchRowRole,
  setBatchRowRole,
  batchRowValue,
  setBatchRowValue,
  handleApplyRowBatch,
  allCandidateModels,
  editingCell,
  setEditingCell,
  cellSearchFilter,
  setCellSearchFilter,
  handleModelChange,
  handleEffortChange,
}: ModelMatrixProps) {
  return matrix.length > 0 ? (
    <div className="overflow-hidden rounded-xl border bg-card shadow-sm">
      <div className="overflow-x-auto">
        <table className="w-full border-collapse text-xs">
          <thead>
            <tr className="border-b bg-muted/40 text-left text-muted-foreground">
              <th className="w-48 px-3.5 py-2.5 font-medium">
                {t("aiWorkflow.role", "角色")}
              </th>
              {SUPPORTED_TOOLS.map((tool) => {
                const degradedError = modelSources?.[tool]?.error;
                const capabilityWarning =
                  modelSources?.[tool]?.capability_warning;
                const isBatchActive = batchColumnTool === tool;
                const candidateModels = modelSources?.[tool]?.models || [];

                return (
                  <th
                    key={tool}
                    data-testid={`column-header-${tool}`}
                    className="px-3.5 py-2.5 font-medium"
                  >
                    <div className="flex flex-col gap-1.5">
                      <div className="flex items-center justify-between gap-2">
                        <span className="uppercase tracking-wider">
                          {tool}
                        </span>
                        <button
                          type="button"
                          data-testid={`batch-fill-column-${tool}`}
                          onClick={() => {
                            setBatchColumnTool(isBatchActive ? null : tool);
                            setBatchColumnValue("");
                          }}
                          className="shrink-0 whitespace-nowrap rounded border bg-background px-2 py-0.5 text-[11px] font-normal text-muted-foreground hover:bg-muted hover:text-foreground"
                        >
                          {t("aiWorkflow.batchFillColumn", "整列填充")}
                        </button>
                      </div>

                      {degradedError ? (
                        <div
                          data-testid={`column-degraded-warning-${tool}`}
                          className="flex items-center gap-1 text-[11px] font-normal text-amber-600 dark:text-amber-400"
                          title={degradedError}
                        >
                          <AlertTriangle className="h-3 w-3 shrink-0" />
                          <span className="truncate">{degradedError}</span>
                        </div>
                      ) : null}

                      {/* Capability-read warning stays separate from the
                          column error and never hides the model candidates. */}
                      {capabilityWarning ? (
                        <div
                          data-testid={`column-capability-warning-${tool}`}
                          className="flex items-center gap-1 text-[11px] font-normal text-amber-600 dark:text-amber-400"
                          title={capabilityWarning}
                        >
                          <AlertTriangle className="h-3 w-3 shrink-0" />
                          <span className="truncate">{capabilityWarning}</span>
                        </div>
                      ) : null}

                      {/* 整列填充展开面板：支持模糊检索、列表选择、防折行按钮与关闭隐藏 */}
                      {isBatchActive ? (
                        <div className="mt-1 flex flex-col gap-1.5 rounded-lg border bg-card p-2 shadow-sm font-normal">
                          <div className="flex items-center justify-between gap-1">
                            <span className="text-[11px] font-medium text-muted-foreground">
                              {t("aiWorkflow.batchFillColumn", "整列填充")}
                            </span>
                            <button
                              type="button"
                              data-testid={`batch-fill-hide-${tool}`}
                              onClick={() => {
                                setBatchColumnTool(null);
                                setBatchColumnValue("");
                              }}
                              title={t(
                                "aiWorkflow.hideBatchFill",
                                "收起隐藏",
                              )}
                              className="rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                            >
                              <X className="h-3.5 w-3.5" />
                            </button>
                          </div>

                          <SearchableModelCombobox
                            value={batchColumnValue}
                            onChange={(val) => setBatchColumnValue(val)}
                            candidates={candidateModels}
                            placeholder={t(
                              "aiWorkflow.modelPlaceholder",
                              "输入或选择模型...",
                            )}
                            testId={`batch-fill-input-${tool}`}
                            autoFocus
                          />

                          <div className="flex items-center justify-end gap-1.5 pt-0.5">
                            <button
                              type="button"
                              onClick={() => {
                                setBatchColumnTool(null);
                                setBatchColumnValue("");
                              }}
                              className="shrink-0 whitespace-nowrap rounded border border-input bg-background px-2 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
                            >
                              {t("aiWorkflow.cancel", "取消")}
                            </button>
                            <button
                              type="button"
                              data-testid={`batch-fill-apply-${tool}`}
                              onClick={() => handleApplyColumnBatch(tool)}
                              className="shrink-0 whitespace-nowrap rounded bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90"
                            >
                              {t("aiWorkflow.apply", "应用")}
                            </button>
                          </div>
                        </div>
                      ) : null}
                    </div>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody className="divide-y divide-border/60">
            {SUPPORTED_ROLES.map((role) => {
              const row = matrix.find((r) => r.role === role);
              const isRowBatchActive = batchRowRole === role;

              return (
                <tr
                  key={role}
                  data-testid={`matrix-row-role-${role}`}
                  className="transition-colors hover:bg-muted/20"
                >
                  <td className="px-3.5 py-2.5 align-top">
                    <div className="flex flex-col gap-1.5">
                      <div className="flex items-center justify-between gap-1">
                        <span
                          className={`inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] font-medium ${ROLE_BADGE_STYLES[role].badge}`}
                        >
                          <span
                            className={`h-1.5 w-1.5 rounded-full ${ROLE_BADGE_STYLES[role].dot}`}
                          />
                          {role}
                        </span>
                        <button
                          type="button"
                          data-testid={`batch-fill-row-${role}`}
                          onClick={() => {
                            setBatchRowRole(isRowBatchActive ? null : role);
                            setBatchRowValue("");
                          }}
                          className="shrink-0 whitespace-nowrap rounded border bg-background px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted hover:text-foreground"
                        >
                          {t("aiWorkflow.batchFillRow", "整行填充")}
                        </button>
                      </div>

                      {isRowBatchActive ? (
                        <div className="mt-1 flex flex-col gap-1.5 rounded-lg border bg-card p-2 shadow-sm font-normal">
                          <div className="flex items-center justify-between gap-1">
                            <span className="text-[11px] font-medium text-muted-foreground">
                              {t("aiWorkflow.batchFillRow", "整行填充")}
                            </span>
                            <button
                              type="button"
                              data-testid={`batch-fill-row-hide-${role}`}
                              onClick={() => {
                                setBatchRowRole(null);
                                setBatchRowValue("");
                              }}
                              title={t(
                                "aiWorkflow.hideBatchFill",
                                "收起隐藏",
                              )}
                              className="rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                            >
                              <X className="h-3.5 w-3.5" />
                            </button>
                          </div>

                          <SearchableModelCombobox
                            value={batchRowValue}
                            onChange={(val) => setBatchRowValue(val)}
                            candidates={allCandidateModels}
                            placeholder={t(
                              "aiWorkflow.modelPlaceholder",
                              "输入或选择模型...",
                            )}
                            testId={`batch-fill-row-input-${role}`}
                            autoFocus
                          />

                          <div className="flex items-center justify-end gap-1.5 pt-0.5">
                            <button
                              type="button"
                              onClick={() => {
                                setBatchRowRole(null);
                                setBatchRowValue("");
                              }}
                              className="shrink-0 whitespace-nowrap rounded border border-input bg-background px-2 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
                            >
                              {t("aiWorkflow.cancel", "取消")}
                            </button>
                            <button
                              type="button"
                              data-testid={`batch-fill-row-apply-${role}`}
                              onClick={() => handleApplyRowBatch(role)}
                              className="shrink-0 whitespace-nowrap rounded bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90"
                            >
                              {t("aiWorkflow.apply", "应用")}
                            </button>
                          </div>
                        </div>
                      ) : null}
                    </div>
                  </td>

                  {SUPPORTED_TOOLS.map((tool) => {
                    const cellData = row?.[tool];
                    const origRow = originalMatrix.find(
                      (r) => r.role === role,
                    );
                    const origCell = origRow?.[tool];
                    const isCellDirty =
                      JSON.stringify(cellData) !== JSON.stringify(origCell);

                    const isCellEditing =
                      editingCell?.role === role &&
                      editingCell?.tool === tool;

                    const candidateModels =
                      modelSources?.[tool]?.models || [];

                    const trimmedModel = cellData?.model?.trim() ?? "";
                    const isCellConfigured = trimmedModel !== "";
                    const capability = isCellConfigured
                      ? getModelCapability(modelSources?.[tool], trimmedModel)
                      : undefined;
                    const writableEfforts =
                      resolveWritableEfforts(capability);
                    const excludedEfforts =
                      resolveExcludedEfforts(capability);
                    const cellEffort = cellData?.reasoning_effort ?? "";
                    const knownIncompatible =
                      !!capability && writableEfforts.length === 0;
                    const knownInvalid =
                      !!capability &&
                      writableEfforts.length > 0 &&
                      !writableEfforts.includes(cellEffort as ValidEffort);
                    const unknownInvalid =
                      !capability &&
                      isCellConfigured &&
                      !isKnownEffort(cellEffort);

                    return (
                      <td
                        key={tool}
                        data-testid={`cell-${role}-${tool}`}
                        onClick={() => {
                          setEditingCell({ role, tool });
                          setCellSearchFilter("");
                        }}
                        className={`px-3.5 py-2.5 align-top transition-colors ${
                          isCellDirty
                            ? "bg-amber-500/10 dark:bg-amber-500/15"
                            : ""
                        }`}
                      >
                        <div className="flex flex-col gap-1.5">
                          {/* 当前值摘要文本，确保 textContent 始终包含 model 与 effort */}
                          <div className="flex flex-wrap items-center gap-1.5">
                            {cellData?.model ? (
                              <span className="font-mono font-medium text-foreground">
                                {cellData.model}
                              </span>
                            ) : (
                              <span className="text-xs text-muted-foreground">
                                {t("aiWorkflow.notSet", "未设置")}
                              </span>
                            )}

                            {cellData?.reasoning_effort ? (
                              <span className="rounded-full border border-border bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
                                {cellData.reasoning_effort}
                              </span>
                            ) : null}
                          </div>

                          {/* 展开编辑区：支持模型输入过滤与候选列表模糊检索 */}
                          {isCellEditing ? (
                            <div
                              className="mt-1 space-y-2 rounded-lg border bg-background p-2.5 shadow-sm"
                              onClick={(e) => e.stopPropagation()}
                            >
                              <div className="flex items-center justify-between border-b pb-1 text-[11px] font-medium text-muted-foreground">
                                <span className="inline-flex items-center gap-1.5">
                                  <span
                                    className={`h-1.5 w-1.5 rounded-full ${getRoleStyle(role).dot}`}
                                  />
                                  {role} · {tool}
                                </span>
                                <button
                                  type="button"
                                  data-testid={`cell-close-${role}-${tool}`}
                                  onClick={() => setEditingCell(null)}
                                  className="rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                                  title={t("aiWorkflow.cancel", "取消")}
                                >
                                  <X className="h-3 w-3" />
                                </button>
                              </div>

                              <div>
                                <label className="mb-1 block text-[11px] font-medium text-muted-foreground">
                                  Model
                                </label>
                                <input
                                  type="text"
                                  data-testid={`manual-model-input-${role}-${tool}`}
                                  value={cellData?.model || ""}
                                  onChange={(e) => {
                                    handleModelChange(
                                      role,
                                      tool,
                                      e.target.value,
                                    );
                                    setCellSearchFilter(e.target.value);
                                  }}
                                  placeholder={t(
                                    "aiWorkflow.modelPlaceholder",
                                    "输入或选择模型...",
                                  )}
                                  className="h-8 w-full rounded border bg-background px-2 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                                />
                              </div>

                              <div>
                                <label className="mb-1 block text-[11px] font-medium text-muted-foreground">
                                  {t(
                                    "aiWorkflow.reasoningEffort",
                                    "推理强度",
                                  )}
                                </label>

                                {capability ? (
                                  <select
                                    data-testid={`effort-select-${role}-${tool}`}
                                    disabled={writableEfforts.length === 0}
                                    value={
                                      writableEfforts.includes(
                                        cellEffort as ValidEffort,
                                      )
                                        ? cellEffort
                                        : ""
                                    }
                                    onChange={(e) =>
                                      handleEffortChange(
                                        role,
                                        tool,
                                        e.target.value,
                                      )
                                    }
                                    className="h-8 w-full rounded border bg-background px-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary disabled:cursor-not-allowed disabled:opacity-50"
                                  >
                                    {writableEfforts.map((eff) => (
                                      <option key={eff} value={eff}>
                                        {eff}
                                      </option>
                                    ))}
                                  </select>
                                ) : (
                                  <input
                                    type="text"
                                    data-testid={`effort-manual-input-${role}-${tool}`}
                                    value={cellEffort}
                                    onChange={(e) =>
                                      handleEffortChange(
                                        role,
                                        tool,
                                        e.target.value,
                                      )
                                    }
                                    placeholder="low | medium | high | xhigh | max | ultra"
                                    className="h-8 w-full rounded border bg-background px-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                                  />
                                )}

                                {excludedEfforts.length > 0 ? (
                                  <p
                                    data-testid={`effort-schema-exclusion-${role}-${tool}`}
                                    className="mt-1 text-[11px] text-amber-600 dark:text-amber-400"
                                  >
                                    {t("aiWorkflow.effortSchemaExcluded", {
                                      values: excludedEfforts.join(", "),
                                      defaultValue:
                                        "Profile schema excludes: {{values}}",
                                    })}
                                  </p>
                                ) : null}

                                {knownIncompatible ? (
                                  <p
                                    data-testid={`effort-incompatibility-${role}-${tool}`}
                                    className="mt-1 text-[11px] text-destructive"
                                  >
                                    {t(
                                      "aiWorkflow.effortIncompatibility",
                                      "This model has no inference effort supported by the profile schema.",
                                    )}
                                  </p>
                                ) : null}

                                {!capability ? (
                                  <p
                                    data-testid={`effort-unknown-hint-${role}-${tool}`}
                                    className="mt-1 text-[11px] text-muted-foreground"
                                  >
                                    {t("aiWorkflow.effortUnknownHint", {
                                      defaultValue:
                                        "Model capability unknown. Enter a schema effort: low, medium, high, xhigh, max, ultra.",
                                    })}
                                  </p>
                                ) : null}

                                {knownInvalid || unknownInvalid ? (
                                  <p
                                    data-testid={`effort-invalid-${role}-${tool}`}
                                    className="mt-1 text-[11px] text-destructive"
                                  >
                                    {t(
                                      "aiWorkflow.effortInvalid",
                                      "Unsupported inference effort for the selected model.",
                                    )}
                                  </p>
                                ) : null}
                              </div>

                              {/* 候选模型列表：支持输入模糊过滤高亮 */}
                              {candidateModels.length > 0 ? (
                                <div>
                                  <div className="mb-1 flex items-center justify-between text-[11px] font-medium text-muted-foreground">
                                    <span>Candidates</span>
                                    {cellSearchFilter.trim() ? (
                                      <span className="text-[11px] text-primary">
                                        过滤中
                                      </span>
                                    ) : null}
                                  </div>
                                  <div
                                    data-testid={`model-options-${tool}`}
                                    className="max-h-28 space-y-1 overflow-y-auto rounded border bg-muted/30 p-1"
                                  >
                                    {candidateModels
                                      .filter((m) => {
                                        if (!cellSearchFilter.trim())
                                          return true;
                                        return m
                                          .toLowerCase()
                                          .includes(
                                            cellSearchFilter
                                              .trim()
                                              .toLowerCase(),
                                          );
                                      })
                                      .map((m) => (
                                        <button
                                          key={m}
                                          type="button"
                                          onClick={() => {
                                            handleModelChange(role, tool, m);
                                            setCellSearchFilter(m);
                                          }}
                                          className={`block w-full truncate rounded px-2 py-1 text-left font-mono text-xs transition-colors ${
                                            cellData?.model === m
                                              ? "bg-primary text-primary-foreground font-medium"
                                              : "hover:bg-muted"
                                          }`}
                                        >
                                          {m}
                                        </button>
                                      ))}
                                    {candidateModels.filter((m) =>
                                      m
                                        .toLowerCase()
                                        .includes(
                                          cellSearchFilter
                                            .trim()
                                            .toLowerCase(),
                                        ),
                                    ).length === 0 ? (
                                      <div className="py-2 text-center text-[11px] text-muted-foreground">
                                        {t(
                                          "aiWorkflow.noMatchingModels",
                                          "无匹配候选（支持自定义输入）",
                                        )}
                                      </div>
                                    ) : null}
                                  </div>
                                </div>
                              ) : null}

                              <div className="flex items-center justify-end gap-1.5 pt-1">
                                <button
                                  type="button"
                                  data-testid={`cell-cancel-${role}-${tool}`}
                                  onClick={() => setEditingCell(null)}
                                  className="shrink-0 rounded border border-input bg-background px-2.5 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
                                >
                                  {t("aiWorkflow.cancel", "取消")}
                                </button>
                                <button
                                  type="button"
                                  data-testid={`cell-done-${role}-${tool}`}
                                  onClick={() => setEditingCell(null)}
                                  className="shrink-0 rounded bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90"
                                >
                                  {t("aiWorkflow.close", "完成")}
                                </button>
                              </div>
                            </div>
                          ) : null}
                        </div>
                      </td>
                    );
                  })}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  ) : isLoading ? (
    <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
      Loading...
    </div>
  ) : null;
}
