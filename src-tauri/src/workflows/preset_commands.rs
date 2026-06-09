#[tauri::command]
pub fn workflows_presets_list() -> Result<ApiOk<Vec<WorkflowPreset>>, String> {
    let mut presets = load_presets()?;
    presets.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    api_ok(presets)
}

#[tauri::command]
pub fn workflows_preset_upsert(
    app: tauri::AppHandle,
    input: WorkflowPresetInput,
) -> Result<ApiOk<WorkflowPreset>, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("workflow preset name is required".to_string());
    }

    let now = now_ts();
    let mut presets = load_presets()?;
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| format!("wf-{}", uuid::Uuid::new_v4()));
    let tool = normalize_tool(input.tool.as_deref().unwrap_or("claude"));
    let provider_id = input.provider_id.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    if let Some(provider_id) = provider_id.as_deref() {
        app_store::validate_service_provider_reference(&tool, provider_id)?;
    }
    let working_dir = input.working_dir.unwrap_or_default().trim().to_string();
    let preset = WorkflowPreset {
        id: id.clone(),
        name: name.to_string(),
        tool,
        working_dir,
        provider_id,
        mcp_server_ids: dedup_non_empty(&input.mcp_server_ids),
        required_skill_ids: dedup_non_empty(&input.required_skill_ids),
        launch_prompt: input.launch_prompt.and_then(|v| {
            let s = v.trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }),
        launch_scope: normalize_launch_scope(input.launch_scope.as_deref()),
        created_at: now,
        updated_at: now,
    };

    if let Some(pos) = presets.iter().position(|p| p.id == id) {
        let created_at = presets[pos].created_at;
        let mut updated = preset.clone();
        updated.created_at = created_at;
        presets[pos] = updated.clone();
        save_presets(&presets)?;
        tauri::async_runtime::spawn(async move {
            let _ = crate::app_store::sync_enqueue(app, "workflow_preset_upsert".to_string()).await;
        });
        return api_ok(updated);
    }

    presets.push(preset.clone());
    save_presets(&presets)?;
    tauri::async_runtime::spawn(async move {
        let _ = crate::app_store::sync_enqueue(app, "workflow_preset_upsert".to_string()).await;
    });
    api_ok(preset)
}

#[tauri::command]
pub fn workflows_preset_delete(
    app: tauri::AppHandle,
    preset_id: String,
) -> Result<ApiOk<Value>, String> {
    let mut presets = load_presets()?;
    let before = presets.len();
    presets.retain(|p| p.id != preset_id);
    if before != presets.len() {
        save_presets(&presets)?;
        tauri::async_runtime::spawn(async move {
            let _ = crate::app_store::sync_enqueue(app, "workflow_preset_delete".to_string()).await;
        });
    }
    api_ok(json!({ "deleted": before != presets.len() }))
}

#[tauri::command]
pub fn workflows_check_dependencies(
    preset_id: String,
) -> Result<ApiOk<WorkflowDependencyState>, String> {
    let presets = load_presets()?;
    let preset = presets
        .iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| "workflow preset not found".to_string())?;
    let deps = detect_dependencies(preset)?;
    api_ok(deps)
}

#[tauri::command]
pub async fn workflows_apply_dependencies(
    app: tauri::AppHandle,
    preset_id: String,
) -> Result<ApiOk<WorkflowDependencyApplyResult>, String> {
    let presets = load_presets()?;
    let preset = presets
        .iter()
        .find(|p| p.id == preset_id)
        .cloned()
        .ok_or_else(|| "workflow preset not found".to_string())?;
    let launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    let result =
        apply_dependencies_for_preset(app, &preset, &launch_scope, &preset.working_dir).await?;
    api_ok(result)
}
