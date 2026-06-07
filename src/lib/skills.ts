import { invokeTyped } from "@/lib/userActions";

export type SkillsInstallScope = "global" | "project";

export function skillsListInstalled<T>(input: {
  model: string;
  scope: SkillsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("skills_list_installed", {
    model: input.model,
    scope: input.scope,
    projectRoot: input.project_root,
  });
}

export function skillsListCatalog<T>(model?: string | null) {
  return invokeTyped<T>("skills_list_catalog", { model });
}

export function skillsRepoList<T>(
  withUpdate = false,
  input?: {
    scope?: SkillsInstallScope;
    project_root?: string | null;
  },
) {
  return invokeTyped<T>(withUpdate ? "skills_repo_list_with_update" : "skills_repo_list", {
    includeUpdate: withUpdate ? undefined : false,
    scope: input?.scope ?? "global",
    projectRoot: input?.project_root ?? null,
  });
}

export function skillsSyncStatusGet<T>() {
  return invokeTyped<T>("skills_sync_status_get");
}

export function skillsRescanMirror() {
  return invokeTyped("skills_rescan_mirror");
}

export function skillsSyncNow() {
  return invokeTyped("skills_sync_now");
}

export function skillsInstall<T>(input: {
  source_id: string;
  skill_ref: string;
  model: string;
  scope: SkillsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("skills_install", { input });
}

export function skillsRepoSetModel<T>(input: {
  repo_key: string;
  model: string;
  enabled: boolean;
  scope: SkillsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("skills_repo_set_model", { input });
}

export function skillsUninstall<T>(input: {
  model: string;
  skill_id: string;
  scope: SkillsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("skills_uninstall", { input });
}

export function skillsRepoDelete<T>(input: { repo_key: string }) {
  return invokeTyped<T>("skills_repo_delete", { input });
}

export function skillsDetailGet<T>(input: {
  model: string;
  skill_id: string;
  scope: SkillsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("skills_detail_get", { input });
}

export function skillsCatalogDetailGet<T>(input: {
  source_id: string;
  skill_ref: string;
}) {
  return invokeTyped<T>("skills_catalog_detail_get", { input });
}

export function skillsRepoDetailGet<T>(input: { repo_key: string }) {
  return invokeTyped<T>("skills_repo_detail_get", { input });
}

export function skillsCatalogOpenFolder<T>(input: {
  source_id: string;
  skill_ref: string;
}) {
  return invokeTyped<T>("skills_catalog_open_folder", { input });
}

export function skillsRepoReloadPreview<T>(input: { repo_key: string }) {
  return invokeTyped<T>("skills_repo_reload_preview", { input });
}

export function skillsRepoReloadApply<T>(input: {
  repo_key: string;
  sync_to_models: boolean;
}) {
  return invokeTyped<T>("skills_repo_reload_apply", { input });
}

export function skillsOpenFolder<T>(input: {
  model: string;
  skill_id: string;
  scope: SkillsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("skills_open_folder", { input });
}

export function skillsRepoImportFolder<T>(input: { folder_path: string }) {
  return invokeTyped<T>("skills_repo_import_folder", { input });
}
