use crate::subagents::{
    acquire_job_key, api_ok, combined_revision, find_definition_markdown, job_lock,
    load_local_subagents_state, load_subagents_state, local_source_id, local_subagent_id,
    parse_subagent_frontmatter_meta, parse_subagent_md, read_required_subagent_dir_name,
    resolve_scan_root, save_subagents_state, sha256_hex, trigger_storage_sync,
    upsert_repo_dir_name, upsert_repository_from_dir, ApiOk, RepoImportFolderInput,
    RepoImportFolderResult,
};
use std::fs;

#[tauri::command]
pub async fn subagents_repo_import_folder(
    app: tauri::AppHandle,
    input: RepoImportFolderInput,
) -> Result<ApiOk<RepoImportFolderResult>, String> {
    let folder_can = resolve_scan_root(&input.folder_path)?;
    let dedupe_key = format!(
        "repo_import_folder:{}",
        sha256_hex(&folder_can.to_string_lossy())
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            return Err("subagents/import_busy".to_string());
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let subagent_md = find_definition_markdown(&folder_can)
        .ok_or_else(|| "subagents/invalid_subagent_dir".to_string())?;
    let md_content = fs::read_to_string(&subagent_md).map_err(|e| e.to_string())?;
    let dir_name = read_required_subagent_dir_name(&folder_can)?;
    let (name, description, declared_models) = parse_subagent_md(&md_content, &[]);
    let (declared_model, declared_tools) = parse_subagent_frontmatter_meta(&md_content);
    let source_id = local_source_id(&folder_can);
    let source_rel_path = ".".to_string();
    let subagent_id = local_subagent_id(&source_id, &source_rel_path);

    let mut shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    let record = upsert_repository_from_dir(
        &mut shared_state,
        &folder_can,
        &source_id,
        &source_rel_path,
        &subagent_id,
        &dir_name,
        "local_import",
        &name,
        &description,
        &declared_models,
        declared_model.as_deref(),
        &declared_tools,
        &source_id,
        Some(folder_can.to_string_lossy().to_string()),
        None,
        false,
    )?;
    let _ = upsert_repo_dir_name(
        &mut shared_state,
        &source_id,
        &source_rel_path,
        &subagent_id,
        &dir_name,
    );
    shared_state = save_subagents_state(shared_state)?;
    trigger_storage_sync(app, "subagents_repo_import_folder");

    let result = RepoImportFolderResult {
        repo_key: record.repo_key,
        subagent_id: record.subagent_id,
        source_id: record.source_id,
        source_rel_path: record.source_rel_path,
    };
    api_ok(result, combined_revision(&shared_state, &local_state))
}
