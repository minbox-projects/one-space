import { emit } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useConfirmDialog } from "../../ConfirmDialogProvider";
import type { WorkspaceCapabilityEntry } from "../../workspaceCapabilityContext";
import { skillsCatalogDetailGet, skillsCatalogOpenFolder, skillsDetailGet, skillsInstall, skillsListCatalog, skillsListInstalled, skillsOpenFolder, skillsRepoDetailGet, skillsRepoList, skillsRepoSetModel, skillsRescanMirror, skillsUninstall } from "@/lib/skills";
import { buildInstallStateFromCatalog, buildInstallTargetFromRepository, buildPartialInstallSummary, matchesRepositoryItem, normalizeSourceNameMap, toggleSelectableModel } from "../helpers/workspaceCapabilityHelpers";
import type {
  CapabilityRepoModelInstallState,
  WorkspaceCapabilityPanelMessage,
  WorkspaceDiscoveryMode,
  WorkspaceSkillModel,
  WorkspaceStorageConfigLite,
} from "../types";
import { invoke } from "@tauri-apps/api/core";

export type WorkspaceSkillRecord = {
  id: string;
  dir_name?: string;
  model: WorkspaceSkillModel;
  models: WorkspaceSkillModel[];
  name: string;
  description: string;
  source_id: string;
  source_rel_path: string;
  installed_at: number;
  updated_at?: number;
  has_update: boolean;
  icon_seed: string;
  scope?: "global" | "project";
  project_root?: string | null;
};

export type WorkspaceCatalogSkill = {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: WorkspaceSkillModel[];
  first_seen_at?: number;
};

export type WorkspaceSkillDetail = {
  skill: WorkspaceSkillRecord;
  markdown: string;
  local_path: string;
};

export type WorkspaceCatalogSkillDetail = {
  skill: WorkspaceCatalogSkill;
  markdown: string;
  source_path: string;
};

export type WorkspaceCatalogOpenFolderResult = {
  repo_key: string;
  opened_path: string;
};

export type WorkspaceRepositorySkillView = {
  repo_key: string;
  skill_id: string;
  dir_name?: string;
  source_id: string;
  source_rel_path: string;
  source_type: string;
  name: string;
  description: string;
  models: WorkspaceSkillModel[];
  icon_seed: string;
  created_at?: number;
  updated_at?: number;
  has_update: boolean;
  installed: CapabilityRepoModelInstallState;
};

export type WorkspaceInstallTargetSkill = {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: WorkspaceSkillModel[];
  repo_key?: string;
  installed?: CapabilityRepoModelInstallState;
};

const modelTabs: { id: WorkspaceSkillModel; label: string }[] = [
  { id: "claude", label: "Claude" },
  { id: "gemini", label: "Gemini" },
  { id: "codex", label: "Codex" },
  { id: "opencode", label: "OpenCode" },
];

function createEmptyInstalledByModel(): Record<WorkspaceSkillModel, WorkspaceSkillRecord[]> {
  return {
    claude: [],
    gemini: [],
    codex: [],
    opencode: [],
  };
}

function formatTs(ts?: number) {
  if (!ts) return "--";
  return new Date(ts * 1000).toLocaleString();
}

function getCapabilityMergeKey(item: Pick<WorkspaceSkillRecord, "id" | "dir_name" | "name" | "source_rel_path">) {
  return String(item.dir_name || item.id || item.source_rel_path.split("/").pop() || item.name || "")
    .trim()
    .toLowerCase();
}

function getSkillScopePriority(skill: WorkspaceSkillRecord) {
  return skill.scope === "project" ? 2 : 1;
}

function mergeInstalledSkillsForModel(skills: WorkspaceSkillRecord[]) {
  const exactSeen = new Set<string>();
  const exact = skills.filter((item) => {
    const key = [item.model, item.id, item.scope || "global", item.project_root || ""].join("::");
    if (exactSeen.has(key)) return false;
    exactSeen.add(key);
    return true;
  });

  const byName = new Map<string, WorkspaceSkillRecord>();
  exact.forEach((skill) => {
    const key = getCapabilityMergeKey(skill);
    if (!key) return;
    const previous = byName.get(key);
    if (!previous || getSkillScopePriority(skill) > getSkillScopePriority(previous)) {
      byName.set(key, skill);
    }
  });
  return Array.from(byName.values());
}

export function useWorkspaceSkillsPanelState(args: {
  rootPath: string;
  isVisible: boolean;
  onNavigateToGlobalPage?: (entry: WorkspaceCapabilityEntry) => void;
}) {
  const { rootPath, isVisible, onNavigateToGlobalPage } = args;
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const normalizedRootPath = rootPath.trim();
  const loadSeqRef = useRef(0);

  const [activeModel, setActiveModel] = useState<WorkspaceSkillModel>("claude");
  const [discoveryMode, setDiscoveryMode] = useState<WorkspaceDiscoveryMode>("recommended");
  const [installedByModel, setInstalledByModel] = useState<Record<WorkspaceSkillModel, WorkspaceSkillRecord[]>>(
    createEmptyInstalledByModel,
  );
  const [catalog, setCatalog] = useState<WorkspaceCatalogSkill[]>([]);
  const [repositorySkills, setRepositorySkills] = useState<WorkspaceRepositorySkillView[]>([]);
  const [sourceNamesById, setSourceNamesById] = useState<Record<string, string>>({});
  const [recommendedSourceFilter, setRecommendedSourceFilter] = useState<"all" | string>("all");
  const [repositorySourceFilter, setRepositorySourceFilter] = useState<"all" | "local" | "remote">("all");
  const [recommendedSearch, setRecommendedSearch] = useState("");
  const [repositorySearch, setRepositorySearch] = useState("");
  const [loading, setLoading] = useState(false);
  const [initialLoadDone, setInitialLoadDone] = useState(false);
  const [message, setMessage] = useState<WorkspaceCapabilityPanelMessage>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailData, setDetailData] = useState<WorkspaceSkillDetail | null>(null);
  const [catalogDetailOpen, setCatalogDetailOpen] = useState(false);
  const [catalogDetailData, setCatalogDetailData] = useState<WorkspaceCatalogSkillDetail | null>(null);
  const [catalogDetailInstallTarget, setCatalogDetailInstallTarget] = useState<WorkspaceInstallTargetSkill | null>(null);
  const [installDialogOpen, setInstallDialogOpen] = useState(false);
  const [installMode, setInstallMode] = useState<"catalog" | "repository">("catalog");
  const [installTarget, setInstallTarget] = useState<WorkspaceInstallTargetSkill | null>(null);
  const [installModels, setInstallModels] = useState<WorkspaceSkillModel[]>([]);
  const [installSubmitting, setInstallSubmitting] = useState(false);
  const [installError, setInstallError] = useState("");
  const [reinstallingKeys, setReinstallingKeys] = useState<Record<string, boolean>>({});

  const groupInstalledByModel = useCallback((skills: WorkspaceSkillRecord[]) => {
    const next = createEmptyInstalledByModel();
    skills.forEach((skill) => {
      next[skill.model].push(skill);
    });
    modelTabs.forEach((tab) => {
      next[tab.id] = mergeInstalledSkillsForModel(next[tab.id]);
    });
    return next;
  }, []);

  const dedupeInstalledSkills = useCallback((skills: WorkspaceSkillRecord[]) => {
    const seen = new Set<string>();
    return skills.filter((item) => {
      const key = [item.model, item.id, item.scope || "global", item.project_root || ""].join("::");
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }, []);

  const fetchInstalledSkills = useCallback(async () => {
    const [globalRes, projectRes] = await Promise.allSettled([
      skillsRescanMirror() as Promise<{ data?: WorkspaceSkillRecord[] }>,
      skillsListInstalled<{ data?: WorkspaceSkillRecord[] }>({
        model: null as unknown as string,
        scope: "project",
        project_root: normalizedRootPath,
      }),
    ]);
    const fallbackGlobalRes = async () =>
      skillsListInstalled<{ data?: WorkspaceSkillRecord[] }>({
        model: null as unknown as string,
        scope: "global",
        project_root: null,
      }).catch(() => null);
    const globalData =
      globalRes.status === "fulfilled" ? ((globalRes.value as { data?: WorkspaceSkillRecord[] }).data || []) : (await fallbackGlobalRes())?.data || [];
    if (projectRes.status !== "fulfilled") {
      throw projectRes.reason;
    }
    return dedupeInstalledSkills([...globalData, ...(projectRes.value.data || [])]);
  }, [dedupeInstalledSkills, normalizedRootPath]);

  const reloadAll = useCallback(async () => {
    const seq = loadSeqRef.current + 1;
    loadSeqRef.current = seq;
    const [installedRes, catalogRes, repositoryRes, configRes] = await Promise.allSettled([
      fetchInstalledSkills(),
      skillsListCatalog<{ data?: WorkspaceCatalogSkill[] }>(null),
      skillsRepoList<{ data?: WorkspaceRepositorySkillView[] }>(false, {
        scope: "project",
        project_root: normalizedRootPath,
      }),
      invoke<WorkspaceStorageConfigLite>("get_storage_config"),
    ]);
    if (seq !== loadSeqRef.current) return;
    if (installedRes.status !== "fulfilled") {
      throw installedRes.reason;
    }

    setInstalledByModel(groupInstalledByModel(installedRes.value || []));
    setCatalog(catalogRes.status === "fulfilled" ? catalogRes.value.data || [] : []);
    setRepositorySkills(repositoryRes.status === "fulfilled" ? repositoryRes.value.data || [] : []);
    setSourceNamesById(configRes.status === "fulfilled" ? normalizeSourceNameMap(configRes.value, "skills_sources") : {});
  }, [fetchInstalledSkills, groupInstalledByModel, normalizedRootPath]);

  useEffect(() => {
    if (!isVisible) return;
    let disposed = false;
    const run = async () => {
      setLoading(true);
      setInitialLoadDone(false);
      setInstalledByModel(createEmptyInstalledByModel());
      try {
        await reloadAll();
      } finally {
        if (!disposed) {
          setInitialLoadDone(true);
          setLoading(false);
        }
      }
    };
    void run().catch((error) => {
      if (!disposed) {
        setMessage({
          type: "error",
          text: t("error", "Error: {{message}}", { message: String(error) }),
        });
        setInitialLoadDone(true);
        setLoading(false);
      }
    });
    return () => {
      disposed = true;
    };
  }, [isVisible, reloadAll, t]);

  useEffect(() => {
    if (!message) return;
    const timer = window.setTimeout(() => setMessage(null), 3000);
    return () => window.clearTimeout(timer);
  }, [message]);

  const installedCounts = useMemo(
    () => ({
      claude: installedByModel.claude.length,
      gemini: installedByModel.gemini.length,
      codex: installedByModel.codex.length,
      opencode: installedByModel.opencode.length,
    }),
    [installedByModel],
  );

  const activeInstalled = useMemo(
    () =>
      [...(installedByModel[activeModel] || [])].sort((a, b) => {
        const aTs = a.updated_at || a.installed_at || 0;
        const bTs = b.updated_at || b.installed_at || 0;
        return bTs - aTs;
      }),
    [activeModel, installedByModel],
  );

  const activeSkillLoadRule = useMemo(
    () =>
      t(
        `workspaceSkillsLoadRule${activeModel === "opencode" ? "OpenCode" : activeModel.charAt(0).toUpperCase() + activeModel.slice(1)}`,
        "OneSpace workspace view merges user-level and directory-level skills. Same-name directory-level skills take precedence; non-conflicting user-level skills remain.",
      ),
    [activeModel, t],
  );

  const installedBySourcePath = useMemo(() => {
    const next = new Map<string, WorkspaceSkillRecord>();
    activeInstalled.forEach((skill) => {
      next.set(`${skill.source_id}:${skill.source_rel_path}`, skill);
    });
    return next;
  }, [activeInstalled]);

  const catalogSources = useMemo(() => {
    const seen = new Set<string>();
    return catalog
      .filter((item) => item.models.includes(activeModel))
      .flatMap((item) => {
        const sourceId = String(item.source_id || "").trim();
        if (!sourceId || seen.has(sourceId)) return [];
        seen.add(sourceId);
        return [{ id: sourceId, label: sourceNamesById[sourceId] || sourceId }];
      });
  }, [activeModel, catalog, sourceNamesById]);

  useEffect(() => {
    if (recommendedSourceFilter === "all") return;
    if (catalogSources.some((source) => source.id === recommendedSourceFilter)) return;
    setRecommendedSourceFilter("all");
  }, [catalogSources, recommendedSourceFilter]);

  const visibleCatalog = useMemo(() => {
    const keyword = recommendedSearch.trim().toLowerCase();
    return catalog.filter((item) => {
      if (!item.models.includes(activeModel)) return false;
      if (recommendedSourceFilter !== "all" && item.source_id !== recommendedSourceFilter) return false;
      if (!keyword) return true;
      return [item.name, item.description, item.id, item.rel_path, item.dir_name, item.source_id].some((field) =>
        String(field || "").toLowerCase().includes(keyword),
      );
    });
  }, [activeModel, catalog, recommendedSearch, recommendedSourceFilter]);

  const visibleRepository = useMemo(() => {
    const keyword = repositorySearch.trim().toLowerCase();
    return repositorySkills.filter((repo) => {
      if (!repo.models.includes(activeModel)) return false;
      if (repositorySourceFilter === "remote" && repo.source_type !== "remote") return false;
      if (repositorySourceFilter === "local" && repo.source_type !== "local_import" && repo.source_type !== "mirror") {
        return false;
      }
      if (!keyword) return true;
      return [repo.name, repo.description, repo.skill_id, repo.source_rel_path, repo.dir_name, repo.source_type].some(
        (field) => String(field || "").toLowerCase().includes(keyword),
      );
    });
  }, [activeModel, repositorySearch, repositorySkills, repositorySourceFilter]);

  const buildInstallStateForCatalog = useCallback(
    (item: WorkspaceCatalogSkill) => buildInstallStateFromCatalog(item, installedByModel),
    [installedByModel],
  );

  const hasInstallableRepoModels = useCallback((target: WorkspaceInstallTargetSkill | null) => {
    if (!target?.installed) return true;
    return target.models.some((model) => !target.installed?.[model]);
  }, []);

  const openInstallDialog = useCallback((target: WorkspaceInstallTargetSkill, mode: "catalog" | "repository") => {
    const allowed = target.models.filter((model) => modelTabs.some((tab) => tab.id === model));
    if (allowed.length === 0) return;
    setInstallMode(mode);
    setInstallTarget(target);
    setInstallModels([allowed.includes(activeModel) ? activeModel : allowed[0]]);
    setInstallError("");
    setInstallDialogOpen(true);
  }, [activeModel]);

  const handleRefresh = useCallback(async () => {
    setLoading(true);
    try {
      await reloadAll();
    } catch (error) {
      setMessage({
        type: "error",
        text: t("error", "Error: {{message}}", { message: String(error) }),
      });
    } finally {
      setLoading(false);
    }
  }, [reloadAll, t]);

  const handleOpenDetail = useCallback(async (skill: WorkspaceSkillRecord) => {
    try {
      const res = await skillsDetailGet<{ data: WorkspaceSkillDetail }>({
        model: skill.model,
        skill_id: skill.id,
        scope: skill.scope || "project",
        project_root: skill.project_root || normalizedRootPath,
      });
      setDetailData(res.data);
      setDetailOpen(true);
    } catch (error) {
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    }
  }, [normalizedRootPath, t]);

  const handleOpenCatalogDetail = useCallback(async (item: WorkspaceCatalogSkill) => {
    try {
      const res = await skillsCatalogDetailGet<{ data: WorkspaceCatalogSkillDetail }>({
        source_id: item.source_id,
        skill_ref: item.rel_path,
      });
      const matchedRepo = repositorySkills.find((repo) =>
        matchesRepositoryItem(
          { ...repo, capability_id: repo.skill_id },
          { source_id: item.source_id, source_rel_path: item.rel_path, id: item.id, dir_name: item.dir_name },
        ),
      );
      setCatalogDetailInstallTarget(
        matchedRepo
          ? buildInstallTargetFromRepository({ ...matchedRepo, capability_id: matchedRepo.skill_id })
          : {
              source_id: item.source_id,
              id: item.id,
              rel_path: item.rel_path,
              dir_name: item.dir_name,
              name: item.name,
              description: item.description,
              models: item.models,
              installed: buildInstallStateForCatalog(item),
            },
      );
      setCatalogDetailData(res.data);
      setCatalogDetailOpen(true);
    } catch (error) {
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    }
  }, [buildInstallStateForCatalog, repositorySkills, t]);

  const handleOpenRepositoryDetail = useCallback(async (repo: WorkspaceRepositorySkillView) => {
    try {
      const res = await skillsRepoDetailGet<{ data: WorkspaceCatalogSkillDetail }>({ repo_key: repo.repo_key });
      setCatalogDetailInstallTarget(buildInstallTargetFromRepository({ ...repo, capability_id: repo.skill_id }));
      setCatalogDetailData(res.data);
      setCatalogDetailOpen(true);
    } catch (error) {
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    }
  }, [t]);

  const handleOpenFolder = useCallback(async (skill: WorkspaceSkillRecord) => {
    try {
      await skillsOpenFolder({
        model: skill.model,
        skill_id: skill.id,
        scope: skill.scope || "project",
        project_root: skill.project_root || normalizedRootPath,
      });
    } catch (error) {
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    }
  }, [normalizedRootPath, t]);

  const handleOpenCatalogFolder = useCallback(async () => {
    if (!catalogDetailData) return;
    try {
      setLoading(true);
      const res = await skillsCatalogOpenFolder<{ data: WorkspaceCatalogOpenFolderResult }>({
        source_id: catalogDetailData.skill.source_id,
        skill_ref: catalogDetailData.skill.rel_path,
      });
      setCatalogDetailInstallTarget((prev) => ({
        source_id: catalogDetailData.skill.source_id,
        id: catalogDetailData.skill.id,
        rel_path: catalogDetailData.skill.rel_path,
        dir_name: catalogDetailData.skill.dir_name,
        name: catalogDetailData.skill.name,
        description: catalogDetailData.skill.description,
        models: catalogDetailData.skill.models,
        repo_key: res.data.repo_key,
        installed: prev?.installed || buildInstallStateForCatalog(catalogDetailData.skill),
      }));
      await reloadAll();
      setMessage({ type: "success", text: t("openFolder", "Open Folder") });
    } catch (error) {
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    } finally {
      setLoading(false);
    }
  }, [buildInstallStateForCatalog, catalogDetailData, reloadAll, t]);

  const closeCatalogDetail = useCallback(() => {
    setCatalogDetailOpen(false);
    setCatalogDetailData(null);
    setCatalogDetailInstallTarget(null);
  }, []);

  const handleInstallFromCatalogDetail = useCallback(() => {
    if (catalogDetailInstallTarget) {
      const nextInstallTarget = catalogDetailInstallTarget;
      closeCatalogDetail();
      openInstallDialog(nextInstallTarget, nextInstallTarget.repo_key ? "repository" : "catalog");
      return;
    }
    if (!catalogDetailData) return;
    const nextInstallTarget = {
      source_id: catalogDetailData.skill.source_id,
      id: catalogDetailData.skill.id,
      rel_path: catalogDetailData.skill.rel_path,
      dir_name: catalogDetailData.skill.dir_name,
      name: catalogDetailData.skill.name,
      description: catalogDetailData.skill.description,
      models: catalogDetailData.skill.models,
      installed: buildInstallStateForCatalog(catalogDetailData.skill),
    };
    closeCatalogDetail();
    openInstallDialog(nextInstallTarget, "catalog");
  }, [buildInstallStateForCatalog, catalogDetailData, catalogDetailInstallTarget, closeCatalogDetail, openInstallDialog]);

  const handleInstallConfirm = useCallback(async () => {
    if (!installTarget || installModels.length === 0) {
      setInstallError(t("sourceModelsRequired", "Select at least one model."));
      return;
    }
    const targetModels = installTarget.models.filter((model) => installModels.includes(model));
    if (targetModels.length === 0) {
      setInstallError(t("sourceModelsRequired", "Select at least one model."));
      return;
    }

    try {
      setInstallSubmitting(true);
      setInstallError("");
      const results = await Promise.allSettled(
        targetModels.map((model) =>
          installMode === "repository"
            ? skillsRepoSetModel({
                repo_key: installTarget.repo_key || "",
                model,
                enabled: true,
                scope: "project",
                project_root: normalizedRootPath,
              })
            : skillsInstall({
                source_id: installTarget.source_id,
                skill_ref: installTarget.rel_path,
                model,
                scope: "project",
                project_root: normalizedRootPath,
              }),
        ),
      );
      const succeeded = targetModels.filter((_, idx) => results[idx].status === "fulfilled");
      const failed = targetModels.filter((_, idx) => results[idx].status === "rejected");
      await reloadAll();
      emit("refresh-counts").catch(() => {});
      if (succeeded.length > 0) setActiveModel(succeeded[0]);
      if (failed.length === 0) {
        setMessage({
          type: "success",
          text:
            succeeded.length === 1
              ? t("installed", "Installed")
              : t("skillsInstallSuccessMulti", "Installed for {{count}} models", { count: succeeded.length }),
        });
        setInstallDialogOpen(false);
        setInstallTarget(null);
        setInstallModels([]);
        return;
      }
      setMessage({
        type: "error",
        text: t("skillsInstallPartialFailed", "Installed {{success}}, failed {{failed}} ({{models}})", buildPartialInstallSummary({
          success: succeeded.length,
          failed: failed.length,
          failedModels: failed,
        })),
      });
    } catch (error) {
      setInstallError(t("error", "Error: {{message}}", { message: String(error) }));
    } finally {
      setInstallSubmitting(false);
    }
  }, [installMode, installModels, installTarget, normalizedRootPath, reloadAll, t]);

  const handleUninstall = useCallback(async (skill: WorkspaceSkillRecord) => {
    const skillScope = skill.scope || "project";
    const ok = await confirmDialog(t("confirmDelete", { name: skill.name }), {
      okLabel: t("ok", "OK"),
      cancelLabel: t("cancel", "Cancel"),
    });
    if (!ok) return;
    try {
      setLoading(true);
      await skillsUninstall({
        model: skill.model,
        skill_id: skill.id,
        scope: skillScope,
        project_root: skillScope === "project" ? skill.project_root || normalizedRootPath : null,
      });
      await reloadAll();
      emit("refresh-counts").catch(() => {});
    } catch (error) {
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    } finally {
      setLoading(false);
    }
  }, [confirmDialog, normalizedRootPath, reloadAll, t]);

  const handleReinstall = useCallback(async (skill: WorkspaceSkillRecord) => {
    const ok = await confirmDialog(t("skillsReinstallConfirm", "是否使用仓库中最新内容重新安装并覆盖？"), {
      okLabel: t("ok", "OK"),
      cancelLabel: t("cancel", "Cancel"),
    });
    if (!ok) return;
    const repo = repositorySkills.find((item) =>
      matchesRepositoryItem(
        { ...item, capability_id: item.skill_id },
        { source_id: skill.source_id, source_rel_path: skill.source_rel_path, id: skill.id, dir_name: skill.dir_name },
      ),
    );
    if (!repo) {
      setMessage({ type: "error", text: t("skillsReinstallRepoNotFound", "Repository snapshot not found for this skill.") });
      return;
    }
    const reinstallKey = `${skill.model}:${skill.id}`;
    setReinstallingKeys((prev) => ({ ...prev, [reinstallKey]: true }));
    try {
      await skillsRepoSetModel({
        repo_key: repo.repo_key,
        model: skill.model,
        enabled: true,
        scope: skill.scope || "project",
        project_root: (skill.scope || "project") === "project" ? skill.project_root || normalizedRootPath : null,
      });
      await reloadAll();
      setMessage({ type: "success", text: t("skillsReinstallSuccess", "Skill reinstalled successfully.") });
    } catch (error) {
      setMessage({
        type: "error",
        text: t("skillsReinstallFailed", "Reinstall failed: {{message}}", { message: String(error) }),
      });
    } finally {
      setReinstallingKeys((prev) => {
        const next = { ...prev };
        delete next[reinstallKey];
        return next;
      });
    }
  }, [confirmDialog, normalizedRootPath, reloadAll, repositorySkills, t]);

  const toggleInstallModel = useCallback((model: WorkspaceSkillModel) => {
    setInstallModels((prev) => toggleSelectableModel(prev, model, installTarget?.models || []));
  }, [installTarget?.models]);

  return {
    normalizedRootPath,
    activeModel,
    setActiveModel,
    discoveryMode,
    setDiscoveryMode,
    installedByModel,
    catalog,
    repositorySkills,
    sourceNamesById,
    recommendedSourceFilter,
    setRecommendedSourceFilter,
    repositorySourceFilter,
    setRepositorySourceFilter,
    recommendedSearch,
    setRecommendedSearch,
    repositorySearch,
    setRepositorySearch,
    loading,
    initialLoadDone,
    message,
    detailOpen,
    setDetailOpen,
    detailData,
    catalogDetailOpen,
    setCatalogDetailOpen,
    closeCatalogDetail,
    catalogDetailData,
    catalogDetailInstallTarget,
    installDialogOpen,
    setInstallDialogOpen,
    installMode,
    installTarget,
    setInstallTarget,
    installModels,
    setInstallModels,
    installSubmitting,
    installError,
    setInstallError,
    reinstallingKeys,
    installedCounts,
    activeInstalled,
    activeSkillLoadRule,
    installedBySourcePath,
    catalogSources,
    visibleCatalog,
    visibleRepository,
    hasInstallableRepoModels,
    openInstallDialog,
    handleRefresh,
    handleOpenDetail,
    handleOpenCatalogDetail,
    handleOpenRepositoryDetail,
    handleOpenFolder,
    handleOpenCatalogFolder,
    handleInstallFromCatalogDetail,
    handleInstallConfirm,
    handleUninstall,
    handleReinstall,
    toggleInstallModel,
    onNavigateToGlobalPage,
    formatTs,
    modelTabs,
    buildInstallStateForCatalog,
    reloadAll,
  };
}
