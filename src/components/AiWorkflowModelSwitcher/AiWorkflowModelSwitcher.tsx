import { useCallback, useEffect, useMemo, useState, type FC } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  Layers,
  Pencil,
  Plus,
  RotateCcw,
  Save,
  Sliders,
  Trash2,
  X,
  Zap,
} from "lucide-react";
import {
  SUPPORTED_ROLES,
  SUPPORTED_TOOLS,
  VALID_EFFORTS,
  activateProfile,
  createProfile,
  deleteProfile,
  getModelSources,
  getProfileMatrix,
  listProfiles,
  renameProfile,
  saveProfile,

  type AgentMatrixRow,
  type ModelSourcesResult,
  type ProfileActivationReport,
  type ProfileSummary,
  type SupportedRole,
  type SupportedTool,
  type ValidEffort,
} from "@/lib/aiWorkflowProfiles";
import { getMoreToolPresentation } from "@/lib/moreToolPresentation";
import { useConfirmDialog } from "../ConfirmDialogProvider";
import { useToast } from "../ToastProvider";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../ui/dialog";
import { SearchableModelCombobox } from "./SearchableModelCombobox";

export interface AiWorkflowModelSwitcherProps {
  homeOverride?: string;
}

const ROLE_BADGE_STYLES: Record<SupportedRole, { badge: string; dot: string }> =
  {
    backend: {
      badge: "border-sky-500/25 bg-sky-500/10 text-sky-700 dark:text-sky-400",
      dot: "bg-sky-500",
    },
    frontend: {
      badge:
        "border-violet-500/25 bg-violet-500/10 text-violet-700 dark:text-violet-400",
      dot: "bg-violet-500",
    },
    test: {
      badge:
        "border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400",
      dot: "bg-emerald-500",
    },
    "documentation-maintainer": {
      badge:
        "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-400",
      dot: "bg-amber-500",
    },
    "file-explorer": {
      badge: "border-cyan-500/25 bg-cyan-500/10 text-cyan-700 dark:text-cyan-400",
      dot: "bg-cyan-500",
    },
    "git-operator": {
      badge: "border-rose-500/25 bg-rose-500/10 text-rose-700 dark:text-rose-400",
      dot: "bg-rose-500",
    },
    researcher: {
      badge:
        "border-indigo-500/25 bg-indigo-500/10 text-indigo-700 dark:text-indigo-400",
      dot: "bg-indigo-500",
    },
    "spec-review": {
      badge: "border-teal-500/25 bg-teal-500/10 text-teal-700 dark:text-teal-400",
      dot: "bg-teal-500",
    },
    "standards-review": {
      badge:
        "border-fuchsia-500/25 bg-fuchsia-500/10 text-fuchsia-700 dark:text-fuchsia-400",
      dot: "bg-fuchsia-500",
    },
  };

const ROLE_BADGE_FALLBACK = {
  badge: "border-border bg-muted text-muted-foreground",
  dot: "bg-muted-foreground",
};

const getRoleStyle = (role: string) =>
  ROLE_BADGE_STYLES[role as SupportedRole] ?? ROLE_BADGE_FALLBACK;

export const AiWorkflowModelSwitcher: FC<AiWorkflowModelSwitcherProps> = ({
  homeOverride,
}) => {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const { pushToast } = useToast();
  const { icon: ToolIcon, iconClassName } = getMoreToolPresentation(
    "ai-workflow-model-switcher",
  );

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
  const [unsavedCreatedProfiles, setUnsavedCreatedProfiles] = useState<Set<string>>(
    () => new Set(),
  );

  // 单元格编辑状态
  const [editingCell, setEditingCell] = useState<{
    role: string;
    tool: SupportedTool;
  } | null>(null);
  const [cellSearchFilter, setCellSearchFilter] = useState<string>("");

  // 整列填充状态
  const [batchColumnTool, setBatchColumnTool] = useState<SupportedTool | null>(
    null,
  );
  const [batchColumnValue, setBatchColumnValue] = useState<string>("");

  // 整行填充状态
  const [batchRowRole, setBatchRowRole] = useState<string | null>(null);
  const [batchRowValue, setBatchRowValue] = useState<string>("");

  // 推理强度批量下拉
  const [isEffortDropdownOpen, setIsEffortDropdownOpen] =
    useState<boolean>(false);
  const [selectedEffort, setSelectedEffort] = useState<ValidEffort>("high");

  // 新建方案 Dialog 状态
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState<boolean>(false);
  const [newProfileName, setNewProfileName] = useState<string>("");
  const [createSourceMode, setCreateSourceMode] = useState<"clone" | "blank">(
    "clone",
  );
  const [createError, setCreateError] = useState<string | null>(null);

  // 编辑方案名称 Dialog 状态
  const [isEditDialogOpen, setIsEditDialogOpen] = useState<boolean>(false);
  const [editProfileName, setEditProfileName] = useState<string>("");
  const [editError, setEditError] = useState<string | null>(null);


  // 聚合所有工具候选模型并集，供整行填充使用
  const allCandidateModels = useMemo(() => {
    const set = new Set<string>();
    if (modelSources) {
      for (const tool of SUPPORTED_TOOLS) {
        for (const m of modelSources[tool]?.models || []) {
          set.add(m);
        }
      }
    }
    return Array.from(set).sort();
  }, [modelSources]);

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

  const handleSaveProfile = async () => {
    if (!selectedProfile) return;
    const profileName = selectedProfile;
    const matrixToSave = JSON.parse(JSON.stringify(matrix)) as AgentMatrixRow[];
    setIsActivating(true);
    setError(null);
    setActivationReport(null);
    try {
      const shouldAsk = unsavedCreatedProfiles.has(profileName);

      await saveProfile(profileName, matrixToSave, homeOverride);
      setOriginalMatrix(matrixToSave);
      setUnsavedCreatedProfiles((previous) => {
        const next = new Set(previous);
        next.delete(profileName);
        return next;
      });

      const shouldActivate = shouldAsk
        ? await confirmDialog(
            t("aiWorkflow.saveNewProfilePrompt", { name: profileName }),
            {
              title: t("aiWorkflow.saveNewProfileTitle", "保存新方案"),
              kind: "info",
              okLabel: t("aiWorkflow.saveNewProfileYes", "是"),
              cancelLabel: t("aiWorkflow.saveNewProfileNo", "否"),
            },
          )
        : false;

      if (!shouldActivate) return;

      const report = await activateProfile(profileName, homeOverride);
      setActiveProfile(report.active_profile || selectedProfile);
      setActivationReport(report);
      setProfiles((prev) =>
        prev.map((p) => ({
          ...p,
          active: p.name === (report.active_profile || profileName),
        })),
      );
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setIsActivating(false);
    }
  };

  // 方案新建
  const handleOpenCreateDialog = () => {
    setNewProfileName("");
    setCreateSourceMode("clone");
    setCreateError(null);
    setIsCreateDialogOpen(true);
  };

  const handleCreateProfileSubmit = async () => {
    const trimmed = newProfileName.trim();
    if (!trimmed) {
      setCreateError(t("aiWorkflow.profileNameRequired", "方案名称不能为空"));
      return;
    }
    const nameRegex = /^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$/i;
    if (!nameRegex.test(trimmed)) {
      setCreateError(
        t(
          "aiWorkflow.profileNameInvalid",
          "方案名称格式无效，仅支持字母、数字、短横线与点",
        ),
      );
      return;
    }
    if (profiles.some((p) => p.name.toLowerCase() === trimmed.toLowerCase())) {
      setCreateError(t("aiWorkflow.profileAlreadyExists", "该方案名称已存在"));
      return;
    }

    try {
      setIsActivating(true);
      const copyFrom =
        createSourceMode === "clone" && selectedProfile
          ? selectedProfile
          : undefined;
      await createProfile(trimmed, copyFrom, homeOverride);
      setUnsavedCreatedProfiles((previous) => new Set(previous).add(trimmed));
      pushToast({
        title: t("aiWorkflow.profileCreated", {
          name: trimmed,
          defaultValue: `方案 "${trimmed}" 创建成功`,
        }),
        kind: "success",
      });
      setIsCreateDialogOpen(false);
      const updated = await listProfiles(homeOverride);
      setProfiles(updated);
      setSelectedProfile(trimmed);
      await loadMatrix(trimmed);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setCreateError(message);
    } finally {
      setIsActivating(false);
    }
  };

  // 方案重命名编辑
  const handleOpenEditDialog = () => {
    if (!selectedProfile) return;
    setEditProfileName(selectedProfile);
    setEditError(null);
    setIsEditDialogOpen(true);
  };

  const handleEditProfileSubmit = async () => {
    if (!selectedProfile) return;
    const trimmed = editProfileName.trim();
    if (!trimmed) {
      setEditError(t("aiWorkflow.profileNameRequired", "方案名称不能为空"));
      return;
    }
    if (trimmed === selectedProfile) {
      setIsEditDialogOpen(false);
      return;
    }
    const nameRegex = /^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$/i;
    if (!nameRegex.test(trimmed)) {
      setEditError(
        t(
          "aiWorkflow.profileNameInvalid",
          "方案名称格式无效，仅支持字母、数字、短横线与点",
        ),
      );
      return;
    }
    if (
      profiles.some(
        (p) =>
          p.name.toLowerCase() === trimmed.toLowerCase() &&
          p.name.toLowerCase() !== selectedProfile.toLowerCase(),
      )
    ) {
      setEditError(t("aiWorkflow.profileAlreadyExists", "该方案名称已存在"));
      return;
    }

    try {
      setIsActivating(true);
      const oldName = selectedProfile;
      await renameProfile(oldName, trimmed, homeOverride);
      setUnsavedCreatedProfiles((previous) => {
        const next = new Set(previous);
        if (next.has(oldName)) {
          next.delete(oldName);
          next.add(trimmed);
        }
        return next;
      });
      pushToast({
        title: t("aiWorkflow.profileRenamed", {
          oldName,
          newName: trimmed,
          defaultValue: `方案 "${oldName}" 已重命名为 "${trimmed}"`,
        }),
        kind: "success",
      });
      setIsEditDialogOpen(false);
      const updated = await listProfiles(homeOverride);
      setProfiles(updated);
      if (activeProfile === oldName) {
        setActiveProfile(trimmed);
      }
      setSelectedProfile(trimmed);
      await loadMatrix(trimmed);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setEditError(message);
    } finally {
      setIsActivating(false);
    }
  };

  // 方案删除
  const handleDeleteProfile = async () => {
    if (!selectedProfile || selectedProfile === activeProfile) return;
    const confirmed = await confirmDialog(
      t("aiWorkflow.confirmDeleteProfileMessage", {
        name: selectedProfile,
        defaultValue: `确定要删除方案 "${selectedProfile}" 吗？此操作不可撤销。`,
      }),
      {
        title: t("aiWorkflow.confirmDeleteProfileTitle", "删除配置方案"),
        kind: "warning",
        okLabel: t("delete", "删除"),
        cancelLabel: t("cancel", "取消"),
      },
    );
    if (!confirmed) return;

    try {
      setIsActivating(true);
      await deleteProfile(selectedProfile, homeOverride);
      setUnsavedCreatedProfiles((previous) => {
        const next = new Set(previous);
        next.delete(selectedProfile);
        return next;
      });
      pushToast({
        title: t("aiWorkflow.profileDeleted", {
          name: selectedProfile,
          defaultValue: `方案 "${selectedProfile}" 已删除`,
        }),
        kind: "success",
      });
      const updated = await listProfiles(homeOverride);
      setProfiles(updated);
      const nextProfile =
        updated.find((p) => p.active)?.name || updated[0]?.name || null;
      setSelectedProfile(nextProfile);
      if (nextProfile) {
        await loadMatrix(nextProfile);
      } else {
        setMatrix([]);
        setOriginalMatrix([]);
      }
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      pushToast({ title: message, kind: "error" });
    } finally {
      setIsActivating(false);
    }
  };

  return (
    <div
      data-testid="ai-workflow-model-switcher"
      className="space-y-6 pb-12 text-foreground"
    >
      {/* 1. 工具顶部标准化标题栏（与其他工具保持一致） */}
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-start gap-3">
          <div className={`rounded-lg p-2 ${iconClassName}`}>
            <ToolIcon className="h-5 w-5" />
          </div>
          <div>
            <h2 className="text-xl font-bold tracking-tight">
              {t("aiWorkflowModelSwitcher", "AI Workflow 模型切换")}
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t(
                "aiWorkflowModelSwitcherDesc",
                "集中管理 subagent 角色模型与推理强度，支持一键切换生效。",
              )}
            </p>
          </div>
        </div>
      </div>

      {/* 2. 方案选择与控制栏 */}
      <div className="rounded-xl border bg-card p-4 shadow-sm">
        {/* 状态层：标题与方案计数 */}
        <div className="flex flex-wrap items-center justify-between gap-3 border-b pb-4">
          <div className="flex items-start gap-2">
            <Layers className="h-5 w-5 text-primary" />
            <div>
              <h3 className="text-sm font-semibold">
                {t("aiWorkflow.selectProfile", "配置方案")}
              </h3>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {t("aiWorkflow.selectProfileHint", "选择要查看和编辑的方案")}
                {profiles.length > 0 ? (
                  <>
                    <span className="mx-1.5">·</span>
                    <span>
                      {t("aiWorkflow.profilesCount", {
                        count: profiles.length,
                        defaultValue: `共 ${profiles.length} 个方案`,
                      })}
                    </span>
                  </>
                ) : null}
              </p>
            </div>
          </div>

          <button
            type="button"
            data-testid="create-profile-trigger"
            onClick={handleOpenCreateDialog}
            className="inline-flex h-8 items-center justify-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
            title={t("aiWorkflow.newProfile", "新建方案")}
          >
            <Plus className="h-3.5 w-3.5" />
            <span>{t("aiWorkflow.newProfile", "新建方案")}</span>
          </button>
        </div>


        {/* Row A：方案芯片 */}
        <div
          role="group"
          aria-label={t("aiWorkflow.selectProfile", "配置方案")}
          className="flex flex-wrap items-center gap-2 pt-4"
        >
          {profiles.map((p) => {
            const isSelected = p.name === selectedProfile;
            const isActive = p.name === activeProfile;
            return (
              <button
                key={p.name}
                type="button"
                data-profile-name={p.name}
                data-active={isActive}
                data-selected={isSelected}
                aria-pressed={isSelected}
                onClick={() => void handleSelectProfile(p.name)}
                title={p.name}
                className={`inline-flex max-w-[16rem] shrink-0 items-center gap-2 rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors ${
                  isSelected
                    ? "border-primary bg-primary text-primary-foreground shadow-sm hover:bg-primary/90"
                    : isActive
                      ? "border-emerald-500/40 bg-emerald-500/5 text-foreground hover:bg-emerald-500/10"
                      : "border-border bg-background text-foreground hover:bg-muted/60"
                }`}
              >
                {isActive ? (
                  <CheckCircle2
                    className={`h-3.5 w-3.5 shrink-0 ${isSelected ? "" : "text-emerald-500"}`}
                  />
                ) : null}
                <span className="truncate">{p.name}</span>
                {p.error ? (
                  <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-destructive" />
                ) : null}
                {isActive ? (
                  <span
                    className={
                      isSelected
                        ? "shrink-0 rounded-full bg-white/20 px-1.5 py-0.5 text-[11px] font-medium"
                        : "shrink-0 rounded-full bg-emerald-500/15 px-1.5 py-0.5 text-[11px] font-medium text-emerald-700 dark:text-emerald-400"
                    }
                  >
                    {t("aiWorkflow.profileActiveTag", "已生效")}
                  </span>
                ) : isSelected && isDirty ? (
                  <span className="inline-flex shrink-0 items-center gap-1 rounded-full bg-white/20 px-1.5 py-0.5 text-[11px] font-medium">
                    <span className="h-1.5 w-1.5 rounded-full bg-amber-400" />
                    {t("aiWorkflow.profileEditingTag", "编辑中")}
                  </span>
                ) : null}
              </button>
            );
          })}

          {profiles.length === 0 ? (
            <p className="text-xs text-muted-foreground">
              {t("aiWorkflow.noProfiles", "未找到可用配置方案")}
            </p>
          ) : null}
        </div>

        {/* Row B：操作按钮 */}
        <div className="mt-3 flex flex-wrap items-center justify-between gap-3 border-t pt-3">
          <div className="flex flex-wrap items-center gap-2">
            {/* 编辑方案按钮 */}
            {selectedProfile ? (
              <button
                type="button"
                data-testid="edit-profile-trigger"
                disabled={isActivating || !selectedProfile}
                onClick={handleOpenEditDialog}
                title={t("aiWorkflow.editProfile", "编辑")}
                className="inline-flex shrink-0 items-center gap-1 rounded-lg border border-border bg-background px-2.5 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-muted disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-background"
              >
                <Pencil className="h-3.5 w-3.5" />
                <span>{t("aiWorkflow.editProfile", "编辑")}</span>
              </button>
            ) : null}

            {/* 删除方案按钮 */}
            {selectedProfile ? (
              <button
                type="button"
                data-testid="delete-profile-trigger"
                disabled={
                  isActivating ||
                  !selectedProfile ||
                  selectedProfile === activeProfile
                }
                onClick={() => void handleDeleteProfile()}
                title={
                  selectedProfile === activeProfile
                    ? t(
                        "aiWorkflow.cannotDeleteActiveProfile",
                        "无法删除当前已激活的方案",
                      )
                    : t("aiWorkflow.deleteProfileConfirm", "删除配置方案")
                }
                className="inline-flex shrink-0 items-center gap-1 rounded-lg border border-border bg-background px-2.5 py-1.5 text-xs font-medium text-destructive transition-colors hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-background"
              >
                <Trash2 className="h-3.5 w-3.5" />
                <span>{t("aiWorkflow.deleteProfile", "删除")}</span>
              </button>
            ) : null}
          </div>

          <div className="flex flex-wrap items-center gap-2">
            {isDirty ? (
              <span
                data-testid="matrix-dirty-indicator"
                className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/20 bg-amber-500/10 px-2.5 py-1 text-xs font-medium text-amber-600 dark:text-amber-400"
              >
                <span className="h-2 w-2 rounded-full bg-amber-500" />
                {t("aiWorkflow.unsavedChanges", "未保存修改")}
              </span>
            ) : null}

            {/* 批量应用 Reasoning Effort */}
            <div className="relative">
              <button
                type="button"
                data-testid="batch-apply-effort-trigger"
                onClick={() => setIsEffortDropdownOpen((prev) => !prev)}
                className="inline-flex items-center gap-1.5 rounded-lg border border-input bg-background px-3 py-1.5 text-xs font-medium hover:bg-muted"
              >
                <Sliders className="h-3.5 w-3.5" />
                <span>
                  {t("aiWorkflow.batchApplyEffort", "批量调整推理强度")}
                </span>
                <ChevronDown className="h-3 w-3 opacity-60" />
              </button>

              {isEffortDropdownOpen ? (
                <div
                  role="listbox"
                  className="absolute right-0 top-full z-20 mt-1 w-44 rounded-lg border bg-popover p-1.5 shadow-lg"
                >
                <div className="mb-1 px-2 py-1 text-[11px] font-medium text-muted-foreground">
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
              aria-label={t("aiWorkflow.activate", "激活")}
              disabled={isActivating || !selectedProfile}
              title={
                isDirty
                  ? t(
                      "aiWorkflow.directActivateWarning",
                      "将忽略未保存的修改，激活已保存的磁盘版本",
                    )
                  : undefined
              }
              onClick={() => void handleDirectActivate()}
              className="inline-flex items-center gap-1.5 rounded-lg border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground shadow-sm hover:bg-muted disabled:opacity-50"
            >
              <Zap className="h-3.5 w-3.5 text-primary" />
              {t("aiWorkflow.activate", "激活")}
            </button>

            <button
              type="button"
              aria-label={t("aiWorkflow.save", "保存")}
              disabled={isActivating || !selectedProfile}
              onClick={() => void handleSaveProfile()}
              className="inline-flex items-center gap-1.5 rounded-lg bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-foreground shadow-sm hover:bg-primary/90 disabled:opacity-50"
            >
              <Save className="h-3.5 w-3.5" />
              {t("aiWorkflow.save", "保存")}
            </button>
          </div>
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
                            <span
                              className={`inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] font-medium ${getRoleStyle(agent.name).badge}`}
                            >
                              <span
                                className={`h-1.5 w-1.5 rounded-full ${getRoleStyle(agent.name).dot}`}
                              />
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
            <table className="w-full border-collapse text-xs">
              <thead>
                <tr className="border-b bg-muted/40 text-left text-muted-foreground">
                  <th className="w-48 px-3.5 py-2.5 font-medium">
                    {t("aiWorkflow.role", "角色")}
                  </th>
                  {SUPPORTED_TOOLS.map((tool) => {
                    const degradedError = modelSources?.[tool]?.error;
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
                                        handleUpdateCell(role, tool, {
                                          model: e.target.value,
                                        });
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
                                                handleUpdateCell(role, tool, {
                                                  model: m,
                                                });
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
      ) : null}

      {/* 新建配置方案 Dialog */}
      <Dialog open={isCreateDialogOpen} onOpenChange={setIsCreateDialogOpen}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>
              {t("aiWorkflow.createProfileTitle", "新建配置方案")}
            </DialogTitle>
            <DialogDescription>
              {t(
                "aiWorkflow.createProfileDesc",
                "为 AI Workflow subagent 矩阵创建一套新的模型与推理强度配置方案。",
              )}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-2 text-sm">
            <div>
              <label className="mb-1 block text-xs font-medium text-muted-foreground">
                {t("aiWorkflow.profileName", "方案名称")}
              </label>
              <input
                type="text"
                data-testid="create-profile-name-input"
                value={newProfileName}
                onChange={(e) => {
                  setNewProfileName(e.target.value);
                  setCreateError(null);
                }}
                placeholder={t(
                  "aiWorkflow.profileNamePlaceholder",
                  "例如 my-new-profile",
                )}
                className="h-8 w-full rounded border bg-background px-2.5 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary"
              />
              <p className="mt-1 text-[11px] text-muted-foreground">
                {t("aiWorkflow.profileNameHelp", "仅支持字母、数字、短横线与点")}
              </p>
            </div>
            <div>
              <label className="mb-1.5 block text-xs font-medium text-muted-foreground">
                {t("aiWorkflow.initialConfigFrom", "初始配置来源")}
              </label>
              <div className="space-y-2">
                <label className="flex cursor-pointer items-center gap-2 text-xs">
                  <input
                    type="radio"
                    name="cloneSource"
                    data-testid="create-profile-radio-clone"
                    checked={createSourceMode === "clone"}
                    onChange={() => setCreateSourceMode("clone")}
                  />
                  <span>
                    {t("aiWorkflow.cloneCurrent", {
                      name: selectedProfile || "当前方案",
                      defaultValue: `从当前方案复制 (${selectedProfile || "当前方案"})`,
                    })}
                  </span>
                </label>
                <label className="flex cursor-pointer items-center gap-2 text-xs">
                  <input
                    type="radio"
                    name="cloneSource"
                    data-testid="create-profile-radio-blank"
                    checked={createSourceMode === "blank"}
                    onChange={() => setCreateSourceMode("blank")}
                  />
                  <span>{t("aiWorkflow.blankProfile", "创建空白方案")}</span>
                </label>
              </div>
            </div>
            {createError && (
              <div
                role="alert"
                className="rounded border border-destructive/20 bg-destructive/10 p-2 text-xs text-destructive"
              >
                {createError}
              </div>
            )}
          </div>
          <DialogFooter>
            <button
              type="button"
              onClick={() => setIsCreateDialogOpen(false)}
              className="rounded border border-input bg-background px-3 py-1.5 text-xs font-medium hover:bg-muted"
            >
              {t("aiWorkflow.cancel", "取消")}
            </button>
            <button
              type="button"
              data-testid="create-profile-submit"
              onClick={() => void handleCreateProfileSubmit()}
              className="rounded bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
            >
              {t("aiWorkflow.create", "创建")}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 编辑配置方案名称 Dialog */}
      <Dialog open={isEditDialogOpen} onOpenChange={setIsEditDialogOpen}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>
              {t("aiWorkflow.editProfileTitle", "编辑方案名称")}
            </DialogTitle>
            <DialogDescription>
              {t(
                "aiWorkflow.editProfileDesc",
                "修改配置方案的名称。",
              )}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-2 text-sm">
            <div>
              <label className="mb-1 block text-xs font-medium text-muted-foreground">
                {t("aiWorkflow.profileName", "方案名称")}
              </label>
              <input
                type="text"
                data-testid="edit-profile-name-input"
                value={editProfileName}
                onChange={(e) => {
                  setEditProfileName(e.target.value);
                  setEditError(null);
                }}
                placeholder={t(
                  "aiWorkflow.profileNamePlaceholder",
                  "例如 my-new-profile",
                )}
                className="h-8 w-full rounded border bg-background px-2.5 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary"
              />
              <p className="mt-1 text-[11px] text-muted-foreground">
                {t("aiWorkflow.profileNameHelp", "仅支持字母、数字、短横线与点")}
              </p>
            </div>
            {editError && (
              <div
                role="alert"
                className="rounded border border-destructive/20 bg-destructive/10 p-2 text-xs text-destructive"
              >
                {editError}
              </div>
            )}
          </div>
          <DialogFooter>
            <button
              type="button"
              onClick={() => setIsEditDialogOpen(false)}
              className="rounded border border-input bg-background px-3 py-1.5 text-xs font-medium hover:bg-muted"
            >
              {t("aiWorkflow.cancel", "取消")}
            </button>
            <button
              type="button"
              data-testid="edit-profile-submit"
              onClick={() => void handleEditProfileSubmit()}
              className="rounded bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
            >
              {t("save", "保存")}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

