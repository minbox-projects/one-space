import type {
  CapabilityRepoModelInstallState,
  ModelId,
  WorkspaceCatalogCapabilityBase,
  WorkspaceInstallTargetBase,
  WorkspaceRepositoryCapabilityBase,
  WorkspaceStorageConfigLite,
} from "../types";

export function getRepositorySourceTypeLabel(
  sourceType: string | null | undefined,
  capabilityType: "skills" | "subagents",
  t: (key: string, defaultValue: string) => string,
) {
  switch (sourceType) {
    case "remote":
      return capabilityType === "skills"
        ? t("skillsSourceTypeRemote", "Recommended Source")
        : t("subagentsSourceTypeRemote", "Recommended Source");
    case "local_import":
      return capabilityType === "skills"
        ? t("skillsSourceTypeLocalImport", "Local Import")
        : t("subagentsSourceTypeLocalImport", "Local Import");
    case "mirror":
      return capabilityType === "skills"
        ? t("skillsSourceTypeMirror", "Mirror")
        : t("subagentsSourceTypeMirror", "Mirror");
    default:
      return sourceType || "-";
  }
}

export function normalizeSourceNameMap(
  config: WorkspaceStorageConfigLite | null | undefined,
  key: "skills_sources" | "subagents_sources",
) {
  const next: Record<string, string> = {};
  (config?.[key] || []).forEach((item) => {
    const sourceId = String(item?.id || "").trim();
    const sourceName = String(item?.name || "").trim();
    if (sourceId) {
      next[sourceId] = sourceName || sourceId;
    }
  });
  return next;
}

export function matchesRepositoryItem(
  repo: Pick<WorkspaceRepositoryCapabilityBase<string>, "source_id" | "source_rel_path" | "dir_name"> & {
    capability_id?: string;
  },
  candidate: { source_id: string; source_rel_path: string; id: string; dir_name?: string },
) {
  if (repo.source_id === candidate.source_id && repo.source_rel_path === candidate.source_rel_path) {
    return true;
  }
  if (repo.capability_id === candidate.id) {
    return true;
  }
  return !!candidate.dir_name && !!repo.dir_name && repo.dir_name === candidate.dir_name;
}

export function buildInstallTargetFromRepository<TModel extends string>(
  repo: WorkspaceRepositoryCapabilityBase<TModel> & { capability_id: string },
): WorkspaceInstallTargetBase<TModel> {
  return {
    source_id: repo.source_id,
    id: repo.capability_id,
    rel_path: repo.source_rel_path,
    dir_name: repo.dir_name,
    name: repo.name,
    description: repo.description,
    models: repo.models,
    repo_key: repo.repo_key,
    installed: repo.installed,
  };
}

export function buildInstallStateFromCatalog<TModel extends ModelId>(
  item: WorkspaceCatalogCapabilityBase<TModel>,
  installedByModel: Record<TModel, Array<{ source_id: string; source_rel_path: string; id: string }>>,
) {
  const next = {} as CapabilityRepoModelInstallState;
  (Object.keys(installedByModel) as TModel[]).forEach((model) => {
    next[model as ModelId] = (installedByModel[model] || []).some(
      (capability) =>
        (capability.source_id === item.source_id && capability.source_rel_path === item.rel_path) ||
        capability.id === item.id,
    );
  });
  return next;
}

export function toggleSelectableModel<TModel extends string>(
  selectedModels: TModel[],
  model: TModel,
  allowedModels: TModel[],
) {
  if (!allowedModels.includes(model)) {
    return selectedModels;
  }
  if (selectedModels.includes(model)) {
    return selectedModels.filter((item) => item !== model);
  }
  return [...selectedModels, model];
}

export function buildPartialInstallSummary(args: {
  success: number;
  failed: number;
  failedModels: string[];
}) {
  return {
    success: args.success,
    failed: args.failed,
    models: args.failedModels.join(", "),
  };
}
