/// 获取所有 MCP 服务器
#[tauri::command]
pub fn get_mcp_servers() -> Result<MCPServersState, String> {
    let (state, keysets) = load_state_with_local_sync()?;
    let _ = sync_local_install_state_with_current_servers(&state, &keysets);
    Ok(state)
}

pub fn get_mcp_servers_count_fast() -> Result<usize, String> {
    let state = load_state()?;
    Ok(state.servers.len())
}

pub(crate) fn save_mcp_server_internal(server: MCPServer) -> Result<(), String> {
    let (mut state, _keysets) = load_state_with_local_sync()?;
    let now = Utc::now();

    if let Some(existing) = state.servers.iter_mut().find(|s| s.id == server.id) {
        // 更新现有服务器
        let mut updated_server = server.clone();
        updated_server.created_at = existing.created_at;
        updated_server.updated_at = now;
        if updated_server.config_key.is_none() {
            updated_server.config_key = existing.config_key.clone();
        }
        *existing = updated_server;
    } else {
        // 新增服务器
        let mut new_server = server.clone();
        new_server.created_at = now;
        new_server.updated_at = now;
        if new_server.id.is_empty() {
            new_server.id = format!("mcp-{}", uuid::Uuid::new_v4());
        }
        state.servers.push(new_server);
    }

    let _ = ensure_server_config_keys(&mut state);
    save_state(&state)?;
    let keysets = model_keysets()?;
    let _ = sync_local_install_state_with_current_servers(&state, &keysets)?;

    Ok(())
}

pub(crate) fn delete_mcp_server_internal(server_id: String) -> Result<(), String> {
    let (mut state, _keysets) = load_state_with_local_sync()?;
    state.servers.retain(|s| s.id != server_id);
    save_state(&state)?;
    let keysets = model_keysets()?;
    let _ = sync_local_install_state_with_current_servers(&state, &keysets)?;

    Ok(())
}

pub(crate) fn link_mcp_to_providers_internal(
    server_id: String,
    provider_ids: Vec<String>,
) -> Result<(), String> {
    let (mut state, _keysets) = load_state_with_local_sync()?;

    if let Some(server) = state.servers.iter_mut().find(|s| s.id == server_id) {
        server.linked_provider_ids = provider_ids;
        server.updated_at = Utc::now();
        save_state(&state)?;
        let keysets = model_keysets()?;
        let _ = sync_local_install_state_with_current_servers(&state, &keysets)?;
    } else {
        return Err("MCP Server not found".to_string());
    }

    Ok(())
}

/// 保存 MCP 服务器（新增或更新）
#[tauri::command]
pub fn save_mcp_server(app: tauri::AppHandle, server: MCPServer) -> Result<(), String> {
    save_mcp_server_internal(server)?;
    trigger_storage_sync(app, "mcp_save_server");
    Ok(())
}

/// 删除 MCP 服务器
#[tauri::command]
pub fn delete_mcp_server(app: tauri::AppHandle, server_id: String) -> Result<(), String> {
    delete_mcp_server_internal(server_id)?;
    trigger_storage_sync(app, "mcp_delete_server");
    Ok(())
}

/// 关联 MCP 服务器到供应商
#[tauri::command]
pub fn link_mcp_to_providers(
    app: tauri::AppHandle,
    server_id: String,
    provider_ids: Vec<String>,
) -> Result<(), String> {
    link_mcp_to_providers_internal(server_id, provider_ids)?;
    trigger_storage_sync(app, "mcp_link_providers");
    Ok(())
}

#[tauri::command]
pub fn get_mcp_model_switch_states() -> Result<HashMap<String, MCPModelSwitchState>, String> {
    let (state, keysets) = load_state_with_local_sync()?;
    sync_local_install_state_with_current_servers(&state, &keysets)
}

#[tauri::command]
pub fn refresh_mcp_local_install_state() -> Result<HashMap<String, MCPModelSwitchState>, String> {
    let (state, _keysets) = load_state_with_local_sync()?;
    refresh_local_install_state_from_cli(&state)
}

#[tauri::command]
pub fn set_mcp_model_switch(
    server_id: String,
    model: String,
    enabled: bool,
) -> Result<MCPModelSwitchState, String> {
    let model = MCPModel::from_str(&model)?;
    let (state, _keysets) = load_state_with_local_sync()?;

    let server = state
        .servers
        .iter()
        .find(|item| item.id == server_id)
        .cloned()
        .ok_or("MCP Server not found".to_string())?;

    let key = server
        .config_key
        .clone()
        .unwrap_or_else(|| slugify_server_name(&server.name));

    apply_model_switch(model, &server, &key, enabled)?;
    let all_switches = refresh_local_install_state_from_cli(&state)?;
    Ok(all_switches
        .get(&server_id)
        .cloned()
        .unwrap_or_else(MCPModelSwitchState::default))
}

async fn build_update_info_for_server(
    client: reqwest::Client,
    server: MCPServer,
    checked_at: u64,
) -> MCPUpdateInfo {
    let mut info = MCPUpdateInfo {
        server_id: server.id.clone(),
        package_name: None,
        current_version: None,
        latest_version: None,
        status: MCPUpdateStatus::Unsupported,
        message: None,
        checked_at,
    };

    let parsed = match parse_server_npm_spec(&server) {
        Some(v) => v,
        None => {
            info.message = Some("Only stdio npx MCP servers are supported in v1".to_string());
            return info;
        }
    };

    info.package_name = Some(parsed.package_name.clone());
    info.current_version = parsed.version.clone();

    let latest = match fetch_npm_latest_version(&client, &parsed.package_name).await {
        Ok(v) => v,
        Err(err) => {
            info.status = MCPUpdateStatus::CheckFailed;
            info.message = Some(err);
            return info;
        }
    };
    info.latest_version = Some(latest.clone());

    match parsed.version {
        None => {
            info.status = MCPUpdateStatus::FloatingLatest;
            info.message = Some("Package is floating and follows latest on next run".to_string());
            info
        }
        Some(current) => {
            match compare_semver_like(&current, &latest) {
                Some(std::cmp::Ordering::Less) => {
                    info.status = MCPUpdateStatus::Updatable;
                    info.message = Some("New latest version is available".to_string());
                }
                Some(_) => {
                    info.status = MCPUpdateStatus::UpToDate;
                    info.message = Some("Already on latest stable".to_string());
                }
                None => {
                    info.status = MCPUpdateStatus::CheckFailed;
                    info.message =
                        Some("Unsupported version format; expected semver-like string".to_string());
                }
            }
            info
        }
    }
}

fn upsert_update_item(items: &mut Vec<MCPUpdateInfo>, next: MCPUpdateInfo) {
    if let Some(existing) = items
        .iter_mut()
        .find(|item| item.server_id == next.server_id)
    {
        *existing = next;
        return;
    }
    items.push(next);
}

async fn run_mcp_updates_check_async() -> Result<Vec<MCPUpdateInfo>, String> {
    let (state, _keysets) = load_state_with_local_sync()?;
    let switches = refresh_local_install_state_from_cli(&state)?;

    let enabled_servers = state
        .servers
        .iter()
        .filter(|server| {
            switches
                .get(&server.id)
                .map(enabled_by_any_model)
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();

    let checked_at = now_ts();
    if enabled_servers.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("onespace-mcp-update-checker/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    const MAX_CONCURRENCY: usize = 4;
    let mut items = Vec::new();
    for chunk in enabled_servers.chunks(MAX_CONCURRENCY) {
        let mut handles = Vec::new();
        for server in chunk {
            let client = client.clone();
            let server = server.clone();
            handles.push(tauri::async_runtime::spawn(async move {
                build_update_info_for_server(client, server, checked_at).await
            }));
        }
        for handle in handles {
            match handle.await {
                Ok(item) => items.push(item),
                Err(err) => {
                    items.push(MCPUpdateInfo {
                        server_id: String::new(),
                        package_name: None,
                        current_version: None,
                        latest_version: None,
                        status: MCPUpdateStatus::CheckFailed,
                        message: Some(format!("task join failed: {}", err)),
                        checked_at,
                    });
                }
            }
        }
    }

    items.retain(|item| !item.server_id.is_empty());
    items.sort_by(|a, b| a.server_id.cmp(&b.server_id));
    Ok(items)
}

#[tauri::command]
pub fn mcp_updates_status_get() -> Result<ApiOk<MCPUpdatesState>, String> {
    let state = load_updates_state()?;
    api_ok(state)
}

#[tauri::command]
pub fn mcp_updates_check_background() -> Result<ApiOk<bool>, String> {
    let job = match acquire_job_key("mcp_updates_check")? {
        Some(v) => v,
        None => return api_ok(false),
    };

    {
        let _guard = job_lock().lock().map_err(|e| e.to_string())?;
        let mut state = load_updates_state()?;
        state.status = "checking".to_string();
        state.last_error = None;
        save_updates_state(&state)?;
    }

    std::thread::spawn(move || {
        let _job = job;
        let result = tauri::async_runtime::block_on(run_mcp_updates_check_async());
        let _ = (|| -> Result<(), String> {
            let _guard = job_lock().lock().map_err(|e| e.to_string())?;
            let mut state = load_updates_state()?;
            match result {
                Ok(items) => {
                    state.status = "done".to_string();
                    state.last_error = None;
                    state.last_checked_at = Some(now_ts());
                    state.items = items;
                }
                Err(err) => {
                    state.status = "error".to_string();
                    state.last_error = Some(err);
                    state.last_checked_at = Some(now_ts());
                }
            }
            save_updates_state(&state)
        })();
    });

    api_ok(true)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MCPUpdateApplyInput {
    pub server_id: String,
}

#[tauri::command]
pub async fn mcp_update_apply(
    app: tauri::AppHandle,
    input: MCPUpdateApplyInput,
) -> Result<ApiOk<MCPUpdateInfo>, String> {
    let dedupe_key = format!("mcp_update_apply:{}", input.server_id);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let fallback = load_updates_state()?
                .items
                .into_iter()
                .find(|item| item.server_id == input.server_id)
                .ok_or("update already running and no cached item found")?;
            return api_ok(fallback);
        }
    };

    let (package_name, latest_version, checked_at) = {
        let (state, _keysets) = load_state_with_local_sync()?;
        let server = state
            .servers
            .iter()
            .find(|item| item.id == input.server_id)
            .cloned()
            .ok_or("MCP Server not found".to_string())?;
        let parsed = parse_server_npm_spec(&server).ok_or(
            "Only stdio npx MCP servers are supported and package must be parseable".to_string(),
        )?;
        let current_version = parsed
            .version
            .clone()
            .ok_or("Floating package has no pinned version to upgrade".to_string())?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("onespace-mcp-update-checker/1.0")
            .build()
            .map_err(|e| e.to_string())?;
        let latest = fetch_npm_latest_version(&client, &parsed.package_name).await?;
        if compare_semver_like(&current_version, &latest).is_none() {
            return Err("Unsupported semver-like format for current or latest version".to_string());
        }
        (parsed.package_name, latest, now_ts())
    };

    let mut state = load_state()?;
    let server = state
        .servers
        .iter_mut()
        .find(|item| item.id == input.server_id)
        .ok_or("MCP Server not found".to_string())?;
    let parsed = parse_server_npm_spec(server).ok_or(
        "Only stdio npx MCP servers are supported and package must be parseable".to_string(),
    )?;
    let current_version = parsed
        .version
        .clone()
        .ok_or("Floating package has no pinned version to upgrade".to_string())?;

    let mut applied = false;
    let mut effective_current = current_version.clone();
    if compare_semver_like(&current_version, &latest_version) == Some(std::cmp::Ordering::Less) {
        if let Some(args) = server.args.as_mut() {
            args[parsed.token_index] = format!("{}@{}", parsed.package_name, latest_version);
        }
        server.updated_at = Utc::now();
        save_state(&state)?;
        let keysets = model_keysets()?;
        let _ = sync_local_install_state_with_current_servers(&state, &keysets)?;
        trigger_storage_sync(app, "mcp_update_apply");
        applied = true;
        effective_current = latest_version.clone();
    }

    let info = MCPUpdateInfo {
        server_id: input.server_id.clone(),
        package_name: Some(package_name),
        current_version: Some(effective_current),
        latest_version: Some(latest_version),
        status: MCPUpdateStatus::UpToDate,
        message: Some(if applied {
            "Upgrade applied".to_string()
        } else {
            "Already on latest stable".to_string()
        }),
        checked_at,
    };

    {
        let _guard = job_lock().lock().map_err(|e| e.to_string())?;
        let mut updates = load_updates_state()?;
        upsert_update_item(&mut updates.items, info.clone());
        updates.status = "done".to_string();
        updates.last_error = None;
        updates.last_checked_at = Some(checked_at);
        save_updates_state(&updates)?;
    }

    api_ok(info)
}

/// 测试命令：解密当前存储的数据（仅用于调试）
#[tauri::command]
pub fn debug_decrypt_all() -> Result<Vec<MCPServer>, String> {
    let mut state = load_state()?;

    // 确保解密
    for server in state.servers.iter_mut() {
        let _ = decrypt_sensitive_data(server);
    }

    Ok(state.servers)
}
