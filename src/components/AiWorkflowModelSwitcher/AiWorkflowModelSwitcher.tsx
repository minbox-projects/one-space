import { useCallback, useEffect, useMemo, useState, type FC } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  Layers,
  RotateCcw,
  Save,
  Zap,
  X,
  Sliders,
} from "lucide-react";
import {
  SUPPORTED_ROLES,
  SUPPORTED_TOOLS,
  VALID_EFFORTS,
  activateProfile,
  getModelSources,
  getProfileMatrix,
  listProfiles,
  saveAndActivateProfile,
  type AgentMatrixRow,
  type ModelSourcesResult,
  type ProfileActivationReport,
  type ProfileSummary,
  type SupportedTool,
  type ValidEffort,
} from "@/lib/aiWorkflowProfiles";

export interface AiWorkflowModelSwitcherProps {
  homeOverride?: string;
}

export const AiWorkflowModelSwitcher: FC<AiWorkflowModelSwitcherProps> = ({
  homeOverride,
}) => {
  const { t } = useTranslation();

  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [selectedProfile, setSelectedProfile] = useState<string | null>(null);
  const [activeProfile, setActiveProfile] = useState<string | null>(null);
  const [matrix, setMatrix] = useState<AgentMatrixRow[]>([]);
  const [originalMatrix, setOriginalMatrix] = useState<AgentMatrixRow[]>([]);
  const [modelSources, setModelSources] = useState<ModelSourcesResult | null>(
    null,
  );
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [isActivating, setIsActivating] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [activationReport, setActivationReport] =
    useState<ProfileActivationReport | null>(null);

  const [editingCell, setEditingCell] = useState<{
    role: string;
    tool: SupportedTool;
  } | null>(null);

  const [batchColumnTool, setBatchColumnTool] = useState<SupportedTool | null>(
    null,
  );
  const [batchColumnValue, setBatchColumnValue] = useState<string>("");

  const [batchRowRole, setBatchRowRole] = useState<string | null>(null);
  const [batchRowValue, setBatchRowValue] = useState<string>("");

  const [isEffortDropdownOpen, setIsEffortDropdownOpen] =
    useState<boolean>(false);
  const [selectedEffort, setSelectedEffort] = useState<ValidEffort>("high");

  const normalizeRows = useCallback(
    (rawRows: AgentMatrixRow[]): AgentMatrixRow[] => {
      const rowMap = new Map(rawRows.map((r) => [r.role, r]));
      return SUPPORTED_ROLES.map((role) => {
        const existing = rowMap.get(role);
        return {
          role,
          codex: existing?.codex ? { ...existing.codex } : undefined,
          claude: existing?.claude ? { ...existing.claude } : undefined,
          opencode: existing?.opencode ? { ...existing.opencode } : undefined,
        };
      });
    },
    [],
  );

  const loadMatrix = useCallback(
    async (profileName: string) => {
      try {
        setError(null);
        const data = await getProfileMatrix(profileName, homeOverride);
        const normalized = normalizeRows(data.rows || []);
        setMatrix(normalized);
        setOriginalMatrix(JSON.parse(JSON.stringify(normalized)));
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        setMatrix([]);
        setOriginalMatrix([]);
      }
    },
    [homeOverride, normalizeRows],
  );

  const loadInitialData = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [profileList, sources] = await Promise.all([
        listProfiles(homeOverride),
        getModelSources(homeOverride),
      ]);
      setProfiles(profileList);
      setModelSources(sources);

      const active = profileList.find((p) => p.active);
      if (active) {
        setActiveProfile(active.name);
      }

      const targetProfile = active ? active.name : profileList[0]?.name;
      if (targetProfile) {
        setSelectedProfile(targetProfile);
        await loadMatrix(targetProfile);
      }
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setIsLoading(false);
    }
  }, [homeOverride, loadMatrix]);

  useEffect(() => {
    void loadInitialData();
  }, [loadInitialData]);

  const handleSelectProfile = async (name: string) => {
    setSelectedProfile(name);
    setActivationReport(null);
    setEditingCell(null);
    setBatchColumnTool(null);
    setBatchRowRole(null);
    await loadMatrix(name);
  };

  const isDirty = useMemo(() => {
    if (matrix.length === 0 && originalMatrix.length === 0) return false;
    return JSON.stringify(matrix) !== JSON.stringify(originalMatrix);
  }, [matrix, originalMatrix]);

  const handleReset = () => {
    setMatrix(JSON.parse(JSON.stringify(originalMatrix)));
    setEditingCell(null);
    setError(null);
  };

  const handleUpdateCell = (
    role: string,
    tool: SupportedTool,
    updates: { model?: string; reasoning_effort?: string },
  ) => {
    setMatrix((prev) =>
      prev.map((row) => {
        if (row.role !== role) return row;
        const current = row[tool] || {
          model: "",
          reasoning_effort: "medium",
        };
        return {
          ...row,
          [tool]: {
            model: updates.model !== undefined ? updates.model : current.model,
            reasoning_effort:
              updates.reasoning_effort !== undefined
                ? updates.reasoning_effort
                : current.reasoning_effort,
          },
        };
      }),
    );
  };

  const handleApplyColumnBatch = (tool: SupportedTool) => {
    if (!batchColumnValue.trim()) return;
    setMatrix((prev) =>
      prev.map((row) => {
        const current = row[tool] || {
          model: "",
          reasoning_effort: "medium",
        };
        return {
          ...row,
          [tool]: {
            ...current,
            model: batchColumnValue.trim(),
          },
        };
      }),
    );
    setBatchColumnTool(null);
    setBatchColumnValue("");
  };

  const handleApplyRowBatch = (role: string) => {
    if (!batchRowValue.trim()) return;
    setMatrix((prev) =>
      prev.map((row) => {
        if (row.role !== role) return row;
        return {
          ...row,
          codex: {
            model: batchRowValue.trim(),
            reasoning_effort: row.codex?.reasoning_effort || "medium",
          },
          claude: {
            model: batchRowValue.trim(),
            reasoning_effort: row.claude?.reasoning_effort || "medium",
          },
          opencode: {
            model: batchRowValue.trim(),
            reasoning_effort: row.opencode?.reasoning_effort || "medium",
          },
        };
      }),
    );
    setBatchRowRole(null);
    setBatchRowValue("");
  };

  const handleApplyEffortBatch = () => {
    setMatrix((prev) =>
      prev.map((row) => ({
        ...row,
        codex: row.codex
          ? { ...row.codex, reasoning_effort: selectedEffort }
          : { model: "", reasoning_effort: selectedEffort },
        claude: row.claude
          ? { ...row.claude, reasoning_effort: selectedEffort }
          : { model: "", reasoning_effort: selectedEffort },
        opencode: row.opencode
          ? { ...row.opencode, reasoning_effort: selectedEffort }
          : { model: "", reasoning_effort: selectedEffort },
      })),
    );
    setIsEffortDropdownOpen(false);
  };

  const handleDirectActivate = async () => {
    if (!selectedProfile) return;
    setIsActivating(true);
    setError(null);
    setActivationReport(null);
    try {
      const report = await activateProfile(selectedProfile, homeOverride);
      setActiveProfile(report.active_profile || selectedProfile);
      setActivationReport(report);
      setProfiles((prev) =>
        prev.map((p) => ({
          ...p,
          active: p.name === (report.active_profile || selectedProfile),
        })),
      );
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setIsActivating(false);
    }
  };

  const handleSaveAndActivate = async () => {
    if (!selectedProfile) return;
    setIsActivating(true);
    setError(null);
    setActivationReport(null);
    try {
      const report = await saveAndActivateProfile(
        selectedProfile,
        matrix,
        homeOverride,
      );
      setActiveProfile(report.active_profile || selectedProfile);
      setOriginalMatrix(JSON.parse(JSON.stringify(matrix)));
      setActivationReport(report);
      setProfiles((prev) =>
        prev.map((p) => ({
          ...p,
          active: p.name === (report.active_profile || selectedProfile),
        })),
      );
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setIsActivating(false);
    }
  };

  return (
    <div
      data-testid="ai-workflow-model-switcher"
      className="space-y-6 pb-12 text-foreground"
    >
      {/* 顶部控制栏 */}
      <div className="flex flex-wrap items-center justify-between gap-4 rounded-xl border bg-card p-4 shadow-sm">
        <div className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2">
            <Layers className="h-5 w-5 text-primary" />
            <span className="text-sm font-semibold">
              {t("aiWorkflow.selectProfile", "配置方案")}:
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            {profiles.map((p) => {
              const isSelected = p.name === selectedProfile;
              return (
                <button
                  key={p.name}
                  type="button"
                  onClick={() => void handleSelectProfile(p.name)}
                  className={`inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors ${
                    isSelected
                      ? "border-primary bg-primary text-primary-foreground shadow-sm"
                      : "border-border bg-background hover:bg-muted"
                  }`}
                >
                  <span>{p.name}</span>
                  {p.error ? (
                    <AlertTriangle className="h-3.5 w-3.5 text-destructive" />
                  ) : null}
                </button>
              );
            })}
          </div>

          {activeProfile ? (
            <span
              data-testid="active-profile-badge"
              className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2.5 py-1 text-xs font-medium text-emerald-600 dark:text-emerald-400"
            >
              <CheckCircle2 className="h-3.5 w-3.5" />
              <span>
                {t("aiWorkflow.activeProfile", "当前激活")}: {activeProfile}
              </span>
            </span>
          ) : null}

          {isDirty ? (
            <span
              data-testid="matrix-dirty-indicator"
              className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/20 bg-amber-500/10 px-2.5 py-1 text-xs font-medium text-amber-600 dark:text-amber-400"
            >
              <span className="h-2 w-2 rounded-full bg-amber-500" />
              {t("aiWorkflow.unsavedChanges", "未保存修改")}
            </span>
          ) : null}
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* 批量应用 Reasoning Effort */}
          <div className="relative">
            <button
              type="button"
              data-testid="batch-apply-effort-trigger"
              onClick={() => setIsEffortDropdownOpen((prev) => !prev)}
              className="inline-flex items-center gap-1.5 rounded-lg border border-input bg-background px-3 py-1.5 text-xs font-medium hover:bg-muted"
            >
              <Sliders className="h-3.5 w-3.5" />
              <span>{t("aiWorkflow.batchApplyEffort", "批量调整推理强度")}</span>
              <ChevronDown className="h-3 w-3 opacity-60" />
            </button>

            {isEffortDropdownOpen ? (
              <div
                role="listbox"
                className="absolute right-0 top-full z-20 mt-1 w-44 rounded-lg border bg-popover p-1.5 shadow-lg"
              >
                <div className="mb-1 px-2 py-1 text-[11px] font-semibold text-muted-foreground">
                  {t("aiWorkflow.reasoningEffort", "推理强度")}
                </div>
                {VALID_EFFORTS.map((effort) => (
                  <button
                    key={effort}
                    role="option"
                    aria-selected={selectedEffort === effort}
                    onClick={() => setSelectedEffort(effort)}
                    className={`flex w-full items-center justify-between rounded px-2 py-1 text-xs transition-colors ${
                      selectedEffort === effort
                        ? "bg-primary text-primary-foreground font-medium"
                        : "hover:bg-muted"
                    }`}
                  >
                    <span>{effort}</span>
                    {selectedEffort === effort ? (
                      <CheckCircle2 className="h-3.5 w-3.5" />
                    ) : null}
                  </button>
                ))}
                <div className="mt-2 border-t pt-1.5">
                  <button
                    type="button"
                    data-testid="batch-apply-effort-submit"
                    onClick={handleApplyEffortBatch}
                    className="w-full rounded bg-primary py-1 text-center text-xs font-medium text-primary-foreground hover:bg-primary/90"
                  >
                    {t("aiWorkflow.apply", "应用")}
                  </button>
                </div>
              </div>
            ) : null}
          </div>

          <button
            type="button"
            aria-label="Reset"
            onClick={handleReset}
            className="inline-flex items-center gap-1.5 rounded-lg border border-input bg-background px-3 py-1.5 text-xs font-medium hover:bg-muted"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            {t("aiWorkflow.resetChanges", "重置")}
          </button>

          <button
            type="button"
            aria-label="Direct Activate"
            disabled={isActivating || !selectedProfile}
            onClick={() => void handleDirectActivate()}
            className="inline-flex items-center gap-1.5 rounded-lg border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground shadow-sm hover:bg-muted disabled:opacity-50"
          >
            <Zap className="h-3.5 w-3.5 text-primary" />
            {t("aiWorkflow.directActivate", "直接激活")}
          </button>

          <button
            type="button"
            aria-label="Save and Activate"
            disabled={isActivating || !selectedProfile}
            onClick={() => void handleSaveAndActivate()}
            className="inline-flex items-center gap-1.5 rounded-lg bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-foreground shadow-sm hover:bg-primary/90 disabled:opacity-50"
          >
            <Save className="h-3.5 w-3.5" />
            {t("aiWorkflow.saveAndActivate", "保存并激活")}
          </button>
        </div>
      </div>

      {/* 错误提示条 */}
      {error ? (
        <div
          role="alert"
          className="rounded-xl border border-destructive/30 bg-destructive/10 p-4 font-mono text-sm text-destructive"
        >
          <div className="flex items-start gap-2">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <div className="whitespace-pre-wrap break-all">{error}</div>
          </div>
        </div>
      ) : null}

      {/* 激活成功报告 */}
      {activationReport ? (
        <div
          data-testid="activation-report"
          className="rounded-xl border border-emerald-500/30 bg-emerald-500/5 p-4 shadow-sm dark:bg-emerald-500/10"
        >
          <div className="flex items-center justify-between gap-2 border-b border-emerald-500/20 pb-2">
            <div className="flex items-center gap-2 text-sm font-semibold text-emerald-600 dark:text-emerald-400">
              <CheckCircle2 className="h-4 w-4" />
              <span>
                {t("aiWorkflow.activationReport", "激活报告")} (
                {activationReport.active_profile})
              </span>
            </div>
            <button
              type="button"
              onClick={() => setActivationReport(null)}
              className="text-muted-foreground hover:text-foreground"
            >
              <X className="h-4 w-4" />
            </button>
          </div>

          <div className="mt-3 space-y-2 text-xs">
            {activationReport.installations?.length === 0 ||
            activationReport.message ? (
              <p className="text-muted-foreground">
                {activationReport.message ||
                  t(
                    "aiWorkflow.noManagedTools",
                    "Profile activated, but no tools are managed.",
                  )}
              </p>
            ) : (
              <div>
                <div className="mb-2 text-muted-foreground">
                  {t("aiWorkflow.hostsCount", {
                    count: activationReport.hosts?.length || 0,
                    defaultValue: `${activationReport.hosts?.length || 0} host(s) updated`,
                  })}
                </div>
                <div className="space-y-3">
                  {activationReport.installations?.map((inst, idx) => (
                    <div
                      key={idx}
                      className="rounded-lg border bg-background/50 p-2.5"
                    >
                      <div className="font-mono font-medium text-foreground">
                        {inst.host}: {inst.agents_directory}
                      </div>
                      <div className="mt-2 grid grid-cols-1 gap-1.5 sm:grid-cols-2 md:grid-cols-3">
                        {inst.agents?.map((agent) => (
                          <div
                            key={agent.name}
                            className="flex items-center justify-between rounded border bg-card px-2 py-1"
                          >
                            <span className="font-semibold text-foreground">
                              {agent.name}
                            </span>
                            <span className="font-mono text-muted-foreground">
                              {agent.model} ({agent.reasoning_effort})
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>
      ) : null}

      {/* 9×3 矩阵表格 */}
      {matrix.length > 0 ? (
        <div className="overflow-hidden rounded-xl border bg-card shadow-sm">
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-left text-sm">
              <thead>
                <tr className="border-b bg-muted/50">
                  <th className="w-48 p-3.5 font-semibold text-foreground">
                    {t("aiWorkflow.role", "角色")}
                  </th>
                  {SUPPORTED_TOOLS.map((tool) => {
                    const degradedError = modelSources?.[tool]?.error;
                    const isBatchActive = batchColumnTool === tool;

                    return (
                      <th
                        key={tool}
                        data-testid={`column-header-${tool}`}
                        className="p-3.5 font-semibold text-foreground"
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
                                setBatchColumnTool(
                                  isBatchActive ? null : tool,
                                );
                                setBatchColumnValue("");
                              }}
                              className="rounded border bg-background px-2 py-0.5 text-[11px] font-normal text-muted-foreground hover:bg-muted hover:text-foreground"
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

                          {isBatchActive ? (
                            <div className="mt-1 flex items-center gap-1 font-normal">
                              <input
                                type="text"
                                data-testid={`batch-fill-input-${tool}`}
                                value={batchColumnValue}
                                onChange={(e) =>
                                  setBatchColumnValue(e.target.value)
                                }
                                placeholder={t(
                                  "aiWorkflow.modelPlaceholder",
                                  "输入模型...",
                                )}
                                className="h-7 w-full rounded border bg-background px-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                              />
                              <button
                                type="button"
                                data-testid={`batch-fill-apply-${tool}`}
                                onClick={() => handleApplyColumnBatch(tool)}
                                className="h-7 rounded bg-primary px-2 text-xs text-primary-foreground hover:bg-primary/90"
                              >
                                {t("aiWorkflow.apply", "应用")}
                              </button>
                            </div>
                          ) : null}
                        </div>
                      </th>
                    );
                  })}
                </tr>
              </thead>
              <tbody className="divide-y">
                {SUPPORTED_ROLES.map((role) => {
                  const row = matrix.find((r) => r.role === role);
                  const isRowBatchActive = batchRowRole === role;

                  return (
                    <tr
                      key={role}
                      data-testid={`matrix-row-role-${role}`}
                      className="transition-colors hover:bg-muted/20"
                    >
                      <td className="p-3.5 align-top">
                        <div className="flex flex-col gap-1.5">
                          <div className="flex items-center justify-between gap-1">
                            <span className="font-medium text-foreground">
                              {role}
                            </span>
                            <button
                              type="button"
                              data-testid={`batch-fill-row-${role}`}
                              onClick={() => {
                                setBatchRowRole(
                                  isRowBatchActive ? null : role,
                                );
                                setBatchRowValue("");
                              }}
                              className="rounded border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground hover:bg-muted hover:text-foreground"
                            >
                              {t("aiWorkflow.batchFillRow", "整行填充")}
                            </button>
                          </div>

                          {isRowBatchActive ? (
                            <div className="mt-1 flex items-center gap-1 font-normal">
                              <input
                                type="text"
                                data-testid={`batch-fill-row-input-${role}`}
                                value={batchRowValue}
                                onChange={(e) =>
                                  setBatchRowValue(e.target.value)
                                }
                                placeholder={t(
                                  "aiWorkflow.modelPlaceholder",
                                  "输入模型...",
                                )}
                                className="h-7 w-full rounded border bg-background px-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                              />
                              <button
                                type="button"
                                data-testid={`batch-fill-row-apply-${role}`}
                                onClick={() => handleApplyRowBatch(role)}
                                className="h-7 rounded bg-primary px-2 text-xs text-primary-foreground hover:bg-primary/90"
                              >
                                {t("aiWorkflow.apply", "应用")}
                              </button>
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
                          JSON.stringify(cellData) !==
                          JSON.stringify(origCell);

                        const isCellEditing =
                          editingCell?.role === role &&
                          editingCell?.tool === tool;

                        const candidateModels =
                          modelSources?.[tool]?.models || [];

                        return (
                          <td
                            key={tool}
                            data-testid={`cell-${role}-${tool}`}
                            onClick={() =>
                              setEditingCell({ role, tool })
                            }
                            className={`p-3 align-top transition-colors ${
                              isCellDirty
                                ? "bg-amber-500/10 dark:bg-amber-500/15"
                                : ""
                            }`}
                          >
                            <div className="flex flex-col gap-1.5">
                              {/* 当前值摘要文本，确保 textContent 始终包含 model 与 effort */}
                              <div className="flex flex-wrap items-center gap-1.5">
                                {cellData?.model ? (
                                  <span className="font-mono text-xs font-semibold text-foreground">
                                    {cellData.model}
                                  </span>
                                ) : (
                                  <span className="text-xs text-muted-foreground">
                                    {t("aiWorkflow.notSet", "未设置")}
                                  </span>
                                )}

                                {cellData?.reasoning_effort ? (
                                  <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                                    {cellData.reasoning_effort}
                                  </span>
                                ) : null}
                              </div>

                              {/* 展开编辑区 */}
                              {isCellEditing ? (
                                <div
                                  className="mt-1 space-y-2 rounded-lg border bg-background p-2.5 shadow-sm"
                                  onClick={(e) => e.stopPropagation()}
                                >
                                  <div>
                                    <label className="mb-1 block text-[11px] font-medium text-muted-foreground">
                                      Model
                                    </label>
                                    <input
                                      type="text"
                                      data-testid={`manual-model-input-${role}-${tool}`}
                                      value={cellData?.model || ""}
                                      onChange={(e) =>
                                        handleUpdateCell(role, tool, {
                                          model: e.target.value,
                                        })
                                      }
                                      placeholder={t(
                                        "aiWorkflow.modelPlaceholder",
                                        "输入或选择模型...",
                                      )}
                                      className="h-8 w-full rounded border bg-background px-2 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                                    />
                                  </div>

                                  <div>
                                    <label className="mb-1 block text-[11px] font-medium text-muted-foreground">
                                      {t("aiWorkflow.reasoningEffort", "推理强度")}
                                    </label>
                                    <select
                                      value={
                                        cellData?.reasoning_effort || "medium"
                                      }
                                      onChange={(e) =>
                                        handleUpdateCell(role, tool, {
                                          reasoning_effort: e.target.value,
                                        })
                                      }
                                      className="h-8 w-full rounded border bg-background px-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                                    >
                                      {VALID_EFFORTS.map((eff) => (
                                        <option key={eff} value={eff}>
                                          {eff}
                                        </option>
                                      ))}
                                    </select>
                                  </div>

                                  {/* 候选模型列表 */}
                                  {candidateModels.length > 0 ? (
                                    <div>
                                      <label className="mb-1 block text-[11px] font-medium text-muted-foreground">
                                        Candidates
                                      </label>
                                      <div
                                        data-testid={`model-options-${tool}`}
                                        className="max-h-28 space-y-1 overflow-y-auto rounded border bg-muted/30 p-1"
                                      >
                                        {candidateModels.map((m) => (
                                          <button
                                            key={m}
                                            type="button"
                                            onClick={() =>
                                              handleUpdateCell(role, tool, {
                                                model: m,
                                              })
                                            }
                                            className={`block w-full truncate rounded px-2 py-1 text-left font-mono text-xs transition-colors ${
                                              cellData?.model === m
                                                ? "bg-primary text-primary-foreground font-medium"
                                                : "hover:bg-muted"
                                            }`}
                                          >
                                            {m}
                                          </button>
                                        ))}
                                      </div>
                                    </div>
                                  ) : null}
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
      ) : null}
    </div>
  );
};
