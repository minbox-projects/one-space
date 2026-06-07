import { emit } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useConfirmDialog } from "../../ConfirmDialogProvider";
import type { WorkspaceCapabilityEntry } from "../../workspaceCapabilityContext";
import {
  subagentsCatalogDetailGet,
  subagentsCatalogOpenFolder,
  subagentsDetailGet,
  subagentsInstall,
  subagentsListCatalog,
  subagentsListInstalled,
  subagentsOpenFolder,
  subagentsRepoDetailGet,
  subagentsRepoList,
  subagentsRepoSetModel,
  subagentsRescanMirror,
  subagentsUninstall,
} from "@/lib/subagents";
import { buildInstallStateFromCatalog, buildInstallTargetFromRepository, buildPartialInstallSummary, matchesRepositoryItem, normalizeSourceNameMap, toggleSelectableModel } from "../helpers/workspaceCapabilityHelpers";
import type { CapabilityRepoModelInstallState, WorkspaceCapabilityPanelMessage, WorkspaceDiscoveryMode, WorkspaceStorageConfigLite, WorkspaceSubagentModel } from "../types";
import { invoke } from "@tauri-apps/api/core";

export type WorkspaceSubagentRecord = {
  id: string;
  dir_name?: string;
  model: WorkspaceSubagentModel;
  models: WorkspaceSubagentModel[];
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

export type WorkspaceCatalogSubagent = {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: WorkspaceSubagentModel[];
  model?: string;
  tools?: string[];
  first_seen_at?: number;
};

export type WorkspaceSubagentDetail = {
  subagent: WorkspaceSubagentRecord;
  markdown: string;
  local_path: string;
};

export type WorkspaceCatalogSubagentDetail = {
  subagent: WorkspaceCatalogSubagent;
  markdown: string;
  source_path: string;
};

export type WorkspaceSubagentCatalogOpenFolderResult = {
  repo_key: string;
  opened_path: string;
};

export type WorkspaceRepositorySubagentView = {
  repo_key: string;
  subagent_id: string;
  dir_name?: string;
  source_id: string;
  source_rel_path: string;
  source_type: string;
  name: string;
  description: string;
  models: WorkspaceSubagentModel[];
  model?: string;
  tools?: string[];
  icon_seed: string;
  created_at?: number;
  updated_at?: number;
  has_update: boolean;
  installed: CapabilityRepoModelInstallState;
};

export type WorkspaceInstallTargetSubagent = {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: WorkspaceSubagentModel[];
  repo_key?: string;
  installed?: CapabilityRepoModelInstallState;
};

const modelTabs: { id: WorkspaceSubagentModel; label: string }[] = [
  { id: "claude", label: "Claude" },
  { id: "gemini", label: "Gemini" },
  { id: "codex", label: "Codex" },
  { id: "opencode", label: "OpenCode" },
];

function createEmptyInstalledByModel(): Record<WorkspaceSubagentModel, WorkspaceSubagentRecord[]> {
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

function getCapabilityMergeKey(item: Pick<WorkspaceSubagentRecord, "id" | "dir_name" | "name" | "source_rel_path">) {
  return String(item.dir_name || item.id || item.source_rel_path.split("/").pop() || item.name || "")
    .trim()
    .toLowerCase();
}

function getSubagentScopePriority(subagent: WorkspaceSubagentRecord) {
  return subagent.scope === "project" ? 2 : 1;
}

function mergeInstalledSubagentsForModel(subagents: WorkspaceSubagentRecord[]) {
  const exactSeen = new Set<string>();
  const exact = subagents.filter((item) => {
    const key = [item.model, item.id, item.scope || "global", item.project_root || ""].join("::");
    if (exactSeen.has(key)) return false;
    exactSeen.add(key);
    return true;
  });

  const byName = new Map<string, WorkspaceSubagentRecord>();
  exact.forEach((subagent) => {
    const key = getCapabilityMergeKey(subagent);
    if (!key) return;
    const previous = byName.get(key);
    if (!previous || getSubagentScopePriority(subagent) > getSubagentScopePriority(previous)) {
      byName.set(key, subagent);
    }
  });
  return Array.from(byName.values());
}

export function useWorkspaceSubagentsPanelState(args: {
  rootPath: string;
  isVisible: boolean;
  onNavigateToGlobalPage?: (entry: WorkspaceCapabilityEntry) => void;
}) {
  const { rootPath, isVisible, onNavigateToGlobalPage } = args;
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const normalizedRootPath = rootPath.trim();
  const loadSeqRef = useRef(0);

  const [activeModel, setActiveModel] = useState<WorkspaceSubagentModel>("claude");
  const [discoveryMode, setDiscoveryMode] = useState<WorkspaceDiscoveryMode>("recommended");
  const [installedByModel, setInstalledByModel] = useState<Record<WorkspaceSubagentModel, WorkspaceSubagentRecord[]>>(
    createEmptyInstalledByModel,
  );
  const [catalog, setCatalog] = useState<WorkspaceCatalogSubagent[]>([]);
  const [repositorySubagents, setRepositorySubagents] = useState<WorkspaceRepositorySubagentView[]>([]);
  const [sourceNamesById, setSourceNamesById] = useState<Record<string, string>>({});
  const [recommendedSourceFilter, setRecommendedSourceFilter] = useState<"all" | string>("all");
  const [repositorySourceFilter, setRepositorySourceFilter] = useState<"all" | "local" | "remote">("all");
  const [recommendedSearch, setRecommendedSearch] = useState("");
  const [repositorySearch, setRepositorySearch] = useState("");
  const [loading, setLoading] = useState(false);
  const [initialLoadDone, setInitialLoadDone] = useState(false);
  const [message, setMessage] = useState<WorkspaceCapabilityPanelMessage>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailData, setDetailData] = useState<WorkspaceSubagentDetail | null>(null);
  const [catalogDetailOpen, setCatalogDetailOpen] = useState(false);
  const [catalogDetailData, setCatalogDetailData] = useState<WorkspaceCatalogSubagentDetail | null>(null);
  const [catalogDetailInstallTarget, setCatalogDetailInstallTarget] = useState<WorkspaceInstallTargetSubagent | null>(null);
  const [installDialogOpen, setInstallDialogOpen] = useState(false);
  const [installMode, setInstallMode] = useState<"catalog" | "repository">("catalog");
  const [installTarget, setInstallTarget] = useState<WorkspaceInstallTargetSubagent | null>(null);
  const [installModels, setInstallModels] = useState<WorkspaceSubagentModel[]>([]);
  const [installSubmitting, setInstallSubmitting] = useState(false);
  const [installError, setInstallError] = useState("");
  const [reinstallingKeys, setReinstallingKeys] = useState<Record<string, boolean>>({});

  const groupInstalledByModel = useCallback((subagents: WorkspaceSubagentRecord[]) => {
    const next = createEmptyInstalledByModel();
    subagents.forEach((subagent) => {
      next[subagent.model].push(subagent);
    });
    modelTabs.forEach((tab) => {
      next[tab.id] = mergeInstalledSubagentsForModel(next[tab.id]);
    });
    return next;
  }, []);

  const dedupeInstalledSubagents = useCallback((subagents: WorkspaceSubagentRecord[]) => {
    const seen = new Set<string>();
    return subagents.filter((item) => {
      const key = [item.model, item.id, item.scope || "global", item.project_root || ""].join("::");
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }, []);

  const fetchInstalledSubagents = useCallback(async () => {
    const [globalRes, projectRes] = await Promise.allSettled([
      subagentsRescanMirror() as Promise<{ data?: WorkspaceSubagentRecord[] }>,
      subagentsListInstalled<{ data?: WorkspaceSubagentRecord[] }>({
        model: null as unknown as string,
        scope: "project",
        project_root: normalizedRootPath,
      }),
    ]);
    const fallbackGlobalRes = async () =>
      subagentsListInstalled<{ data?: WorkspaceSubagentRecord[] }>({
        model: null as unknown as string,
        scope: "global",
        project_root: null,
      }).catch(() => null);
    const globalData =
      globalRes.status === "fulfilled"
        ? ((globalRes.value as { data?: WorkspaceSubagentRecord[] }).data || [])
        : (await fallbackGlobalRes())?.data || [];
    if (projectRes.status !== "fulfilled") {
      throw projectRes.reason;
    }
    return dedupeInstalledSubagents([...globalData, ...(projectRes.value.data || [])]);
  }, [dedupeInstalledSubagents, normalizedRootPath]);

  const reloadAll = useCallback(async () => {
    const seq = loadSeqRef.current + 1;
    loadSeqRef.current = seq;
    const [installedRes, catalogRes, repositoryRes, configRes] = await Promise.allSettled([
      fetchInstalledSubagents(),
      subagentsListCatalog<{ data?: WorkspaceCatalogSubagent[] }>(null),
      subagentsRepoList<{ data?: WorkspaceRepositorySubagentView[] }>(false, {
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
    setRepositorySubagents(repositoryRes.status === "fulfilled" ? repositoryRes.value.data || [] : []);
    setSourceNamesById(configRes.status === "fulfilled" ? normalizeSourceNameMap(configRes.value, "subagents_sources") : {});
  }, [fetchInstalledSubagents, groupInstalledByModel, normalizedRootPath]);

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
        setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
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

  const activeSubagentLoadRule = useMemo(() => {
    switch (activeModel) {
      case "claude":
        return t("workspaceSubagentsLoadRuleClaude", "Claude Code subagent precedence is managed > CLI flag > project > user > plugin.");
      case "gemini":
        return t("workspaceSubagentsLoadRuleGemini", "Gemini discovers project agents in .gemini/agents and personal agents in ~/.gemini/agents. Keep same-name agents aligned with the active Gemini CLI discovery rules.");
      case "codex":
        return t("workspaceSubagentsLoadRuleCodex", "Codex custom agents follow config layering. In trusted projects, the closest .codex/config.toml overrides same-name user config keys.");
      case "opencode":
      default:
        return t("workspaceSubagentsLoadRuleOpenCode", "OpenCode merges global and project agent config. Project opencode.json/.opencode agents override global config only for conflicting keys.");
    }
  }, [activeModel, t]);

  const installedBySourcePath = useMemo(() => {
    const next = new Map<string, WorkspaceSubagentRecord>();
    activeInstalled.forEach((subagent) => {
      next.set(`${subagent.source_id}:${subagent.source_rel_path}`, subagent);
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
    return repositorySubagents.filter((repo) => {
      if (!repo.models.includes(activeModel)) return false;
      if (repositorySourceFilter === "remote" && repo.source_type !== "remote") return false;
      if (repositorySourceFilter === "local" && repo.source_type !== "local_import" && repo.source_type !== "mirror") {
        return false;
      }
      if (!keyword) return true;
      return [repo.name, repo.description, repo.subagent_id, repo.source_rel_path, repo.dir_name, repo.source_type].some(
        (field) => String(field || "").toLowerCase().includes(keyword),
      );
    });
  }, [activeModel, repositorySearch, repositorySourceFilter, repositorySubagents]);

  const buildInstallStateForCatalog = useCallback(
    (item: WorkspaceCatalogSubagent) => buildInstallStateFromCatalog(item, installedByModel),
    [installedByModel],
  );

  const hasInstallableRepoModels = useCallback((target: WorkspaceInstallTargetSubagent | null) => {
    if (!target?.installed) return true;
    return target.models.some((model) => !target.installed?.[model]);
  }, []);

  const openInstallDialog = useCallback((target: WorkspaceInstallTargetSubagent, mode: "catalog" | "repository") => {
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
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    } finally {
      setLoading(false);
    }
  }, [reloadAll, t]);

  const handleOpenDetail = useCallback(async (subagent: WorkspaceSubagentRecord) => {
    try {
      const res = await subagentsDetailGet<{ data: WorkspaceSubagentDetail }>({
        model: subagent.model,
        subagent_id: subagent.id,
        scope: subagent.scope || "project",
        project_root: subagent.project_root || normalizedRootPath,
      });
      setDetailData(res.data);
      setDetailOpen(true);
    } catch (error) {
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    }
  }, [normalizedRootPath, t]);

  const handleOpenCatalogDetail = useCallback(async (item: WorkspaceCatalogSubagent) => {
    try {
      const res = await subagentsCatalogDetailGet<{ data: WorkspaceCatalogSubagentDetail }>({
        source_id: item.source_id,
        subagent_ref: item.rel_path,
      });
      const matchedRepo = repositorySubagents.find((repo) =>
        matchesRepositoryItem(
          { ...repo, capability_id: repo.subagent_id },
          { source_id: item.source_id, source_rel_path: item.rel_path, id: item.id, dir_name: item.dir_name },
        ),
      );
      setCatalogDetailInstallTarget(
        matchedRepo
          ? buildInstallTargetFromRepository({ ...matchedRepo, capability_id: matchedRepo.subagent_id })
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
  }, [buildInstallStateForCatalog, repositorySubagents, t]);

  const handleOpenRepositoryDetail = useCallback(async (repo: WorkspaceRepositorySubagentView) => {
    try {
      const res = await subagentsRepoDetailGet<{ data: WorkspaceCatalogSubagentDetail }>({ repo_key: repo.repo_key });
      setCatalogDetailInstallTarget(buildInstallTargetFromRepository({ ...repo, capability_id: repo.subagent_id }));
      setCatalogDetailData(res.data);
      setCatalogDetailOpen(true);
    } catch (error) {
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    }
  }, [t]);

  const handleOpenFolder = useCallback(async (subagent: WorkspaceSubagentRecord) => {
    try {
      await subagentsOpenFolder({
        model: subagent.model,
        subagent_id: subagent.id,
        scope: subagent.scope || "project",
        project_root: subagent.project_root || normalizedRootPath,
      });
    } catch (error) {
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    }
  }, [normalizedRootPath, t]);

  const handleOpenCatalogFolder = useCallback(async () => {
    if (!catalogDetailData) return;
    try {
      setLoading(true);
      const res = await subagentsCatalogOpenFolder<{ data: WorkspaceSubagentCatalogOpenFolderResult }>({
        source_id: catalogDetailData.subagent.source_id,
        subagent_ref: catalogDetailData.subagent.rel_path,
      });
      setCatalogDetailInstallTarget((prev) => ({
        source_id: catalogDetailData.subagent.source_id,
        id: catalogDetailData.subagent.id,
        rel_path: catalogDetailData.subagent.rel_path,
        dir_name: catalogDetailData.subagent.dir_name,
        name: catalogDetailData.subagent.name,
        description: catalogDetailData.subagent.description,
        models: catalogDetailData.subagent.models,
        repo_key: res.data.repo_key,
        installed: prev?.installed || buildInstallStateForCatalog(catalogDetailData.subagent),
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
      source_id: catalogDetailData.subagent.source_id,
      id: catalogDetailData.subagent.id,
      rel_path: catalogDetailData.subagent.rel_path,
      dir_name: catalogDetailData.subagent.dir_name,
      name: catalogDetailData.subagent.name,
      description: catalogDetailData.subagent.description,
      models: catalogDetailData.subagent.models,
      installed: buildInstallStateForCatalog(catalogDetailData.subagent),
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
            ? subagentsRepoSetModel({
                repo_key: installTarget.repo_key || "",
                model,
                enabled: true,
                scope: "project",
                project_root: normalizedRootPath,
              })
            : subagentsInstall({
                source_id: installTarget.source_id,
                subagent_ref: installTarget.rel_path,
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
              : t("subagentsInstallSuccessMulti", "Installed for {{count}} models", { count: succeeded.length }),
        });
        setInstallDialogOpen(false);
        setInstallTarget(null);
        setInstallModels([]);
        return;
      }
      setMessage({
        type: "error",
        text: t("subagentsInstallPartialFailed", "Installed {{success}}, failed {{failed}} ({{models}})", buildPartialInstallSummary({
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

  const handleUninstall = useCallback(async (subagent: WorkspaceSubagentRecord) => {
    const subagentScope = subagent.scope || "project";
    const ok = await confirmDialog(t("confirmDelete", { name: subagent.name }), {
      okLabel: t("ok", "OK"),
      cancelLabel: t("cancel", "Cancel"),
    });
    if (!ok) return;
    try {
      setLoading(true);
      await subagentsUninstall({
        model: subagent.model,
        subagent_id: subagent.id,
        scope: subagentScope,
        project_root: subagentScope === "project" ? subagent.project_root || normalizedRootPath : null,
      });
      await reloadAll();
      emit("refresh-counts").catch(() => {});
    } catch (error) {
      setMessage({ type: "error", text: t("error", "Error: {{message}}", { message: String(error) }) });
    } finally {
      setLoading(false);
    }
  }, [confirmDialog, normalizedRootPath, reloadAll, t]);

  const handleReinstall = useCallback(async (subagent: WorkspaceSubagentRecord) => {
    const ok = await confirmDialog(t("subagentsReinstallConfirm", "是否使用仓库中最新内容重新安装并覆盖？"), {
      okLabel: t("ok", "OK"),
      cancelLabel: t("cancel", "Cancel"),
    });
    if (!ok) return;
    const repo = repositorySubagents.find((item) =>
      matchesRepositoryItem(
        { ...item, capability_id: item.subagent_id },
        { source_id: subagent.source_id, source_rel_path: subagent.source_rel_path, id: subagent.id, dir_name: subagent.dir_name },
      ),
    );
    if (!repo) {
      setMessage({ type: "error", text: t("subagentsReinstallRepoNotFound", "Repository snapshot not found for this subagent.") });
      return;
    }
    const reinstallKey = `${subagent.model}:${subagent.id}`;
    setReinstallingKeys((prev) => ({ ...prev, [reinstallKey]: true }));
    try {
      await subagentsRepoSetModel({
        repo_key: repo.repo_key,
        model: subagent.model,
        enabled: true,
        scope: subagent.scope || "project",
        project_root: (subagent.scope || "project") === "project" ? subagent.project_root || normalizedRootPath : null,
      });
      await reloadAll();
      setMessage({ type: "success", text: t("subagentsReinstallSuccess", "Subagent reinstalled successfully.") });
    } catch (error) {
      setMessage({
        type: "error",
        text: t("subagentsReinstallFailed", "Reinstall failed: {{message}}", { message: String(error) }),
      });
    } finally {
      setReinstallingKeys((prev) => {
        const next = { ...prev };
        delete next[reinstallKey];
        return next;
      });
    }
  }, [confirmDialog, normalizedRootPath, reloadAll, repositorySubagents, t]);

  const toggleInstallModel = useCallback((model: WorkspaceSubagentModel) => {
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
    repositorySubagents,
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
    activeSubagentLoadRule,
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
  };
}
