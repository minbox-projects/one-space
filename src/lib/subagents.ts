import { invokeTyped } from "@/lib/userActions";

export type SubagentsInstallScope = "global" | "project";

export function subagentsListInstalled<T>(input: {
  model: string;
  scope: SubagentsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("subagents_list_installed", {
    model: input.model,
    scope: input.scope,
    projectRoot: input.project_root,
  });
}

export function subagentsListCatalog<T>(model?: string | null) {
  return invokeTyped<T>("subagents_list_catalog", { model });
}

export function subagentsRepoList<T>(
  withUpdate = false,
  input?: {
    scope?: SubagentsInstallScope;
    project_root?: string | null;
  },
) {
  return invokeTyped<T>(
    withUpdate ? "subagents_repo_list_with_update" : "subagents_repo_list",
    {
      includeUpdate: withUpdate ? undefined : false,
      scope: input?.scope ?? "global",
      projectRoot: input?.project_root ?? null,
    },
  );
}

export function subagentsSyncStatusGet<T>() {
  return invokeTyped<T>("subagents_sync_status_get");
}

export function subagentsRescanMirror() {
  return invokeTyped("subagents_rescan_mirror");
}

export function subagentsSyncNow() {
  return invokeTyped("subagents_sync_now");
}

export function subagentsInstall<T>(input: {
  source_id: string;
  subagent_ref: string;
  model: string;
  scope: SubagentsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("subagents_install", { input });
}

export function subagentsRepoSetModel<T>(input: {
  repo_key: string;
  model: string;
  enabled: boolean;
  scope: SubagentsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("subagents_repo_set_model", { input });
}

export function subagentsUninstall<T>(input: {
  model: string;
  subagent_id: string;
  scope: SubagentsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("subagents_uninstall", { input });
}

export function subagentsRepoDelete<T>(input: { repo_key: string }) {
  return invokeTyped<T>("subagents_repo_delete", { input });
}

export function subagentsDetailGet<T>(input: {
  model: string;
  subagent_id: string;
  scope: SubagentsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("subagents_detail_get", { input });
}

export function subagentsCatalogDetailGet<T>(input: {
  source_id: string;
  subagent_ref: string;
}) {
  return invokeTyped<T>("subagents_catalog_detail_get", { input });
}

export function subagentsRepoDetailGet<T>(input: { repo_key: string }) {
  return invokeTyped<T>("subagents_repo_detail_get", { input });
}

export function subagentsCatalogOpenFolder<T>(input: {
  source_id: string;
  subagent_ref: string;
}) {
  return invokeTyped<T>("subagents_catalog_open_folder", { input });
}

export function subagentsUpdateDiffPreview<T>(input: {
  model: string;
  subagent_id: string;
  scope: SubagentsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("subagents_update_diff_preview", { input });
}

export function subagentsRepoReloadPreview<T>(input: { repo_key: string }) {
  return invokeTyped<T>("subagents_repo_reload_preview", { input });
}

export function subagentsRepoReloadApply<T>(input: {
  repo_key: string;
  sync_to_models: boolean;
}) {
  return invokeTyped<T>("subagents_repo_reload_apply", { input });
}

export function subagentsUpdateApply<T>(input: {
  model: string;
  subagent_id: string;
  scope: SubagentsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("subagents_update_apply", { input });
}

export function subagentsOpenFolder<T>(input: {
  model: string;
  subagent_id: string;
  scope: SubagentsInstallScope;
  project_root?: string | null;
}) {
  return invokeTyped<T>("subagents_open_folder", { input });
}

export function subagentsRepoImportFolder<T>(input: { folder_path: string }) {
  return invokeTyped<T>("subagents_repo_import_folder", { input });
}
