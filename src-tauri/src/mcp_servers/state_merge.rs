use super::{
    atomic_write, build_model_switch_state, decrypt_sensitive_data, display_name_from_key,
    encrypt_sensitive_data, get_legacy_mcp_servers_path, get_mcp_servers_path,
    load_local_install_state, model_keysets, read_local_model_configs, save_local_install_state,
    LocalModelConfigs, MCPLocalInstallState, MCPModel, MCPModelSwitchState, MCPServer,
    MCPServersState, ModelKeysets,
};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::fs::{self};

pub(in crate::mcp_servers) fn enabled_by_any_model(state: &MCPModelSwitchState) -> bool {
    state.claude || state.codex || state.gemini || state.opencode
}

pub(in crate::mcp_servers) fn derive_switch_states(
    servers: &[MCPServer],
    keysets: &ModelKeysets,
) -> HashMap<String, MCPModelSwitchState> {
    let mut out = HashMap::new();
    for server in servers {
        let key = server
            .config_key
            .clone()
            .unwrap_or_else(|| slugify_server_name(&server.name));
        out.insert(server.id.clone(), build_model_switch_state(&key, keysets));
    }
    out
}

pub(in crate::mcp_servers) fn normalize_local_install_state(
    servers: &[MCPServer],
    mut state: MCPLocalInstallState,
    defaults: &HashMap<String, MCPModelSwitchState>,
) -> MCPLocalInstallState {
    let server_ids = servers.iter().map(|s| s.id.clone()).collect::<HashSet<_>>();
    state
        .model_switches
        .retain(|server_id, _| server_ids.contains(server_id));
    for server in servers {
        if !state.model_switches.contains_key(&server.id) {
            if let Some(default_state) = defaults.get(&server.id) {
                state
                    .model_switches
                    .insert(server.id.clone(), default_state.clone());
            } else {
                state
                    .model_switches
                    .insert(server.id.clone(), MCPModelSwitchState::default());
            }
        }
    }
    state
}

pub(in crate::mcp_servers) fn slugify_server_name(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "server".to_string()
    } else {
        trimmed
    }
}

pub(in crate::mcp_servers) fn short_suffix(id: &str) -> String {
    let suffix = id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(6)
        .collect::<String>();
    if suffix.is_empty() {
        "mcp".to_string()
    } else {
        suffix.to_lowercase()
    }
}

pub(in crate::mcp_servers) fn unique_config_key(
    base: &str,
    server_id: &str,
    used: &HashSet<String>,
) -> String {
    if !used.contains(base) {
        return base.to_string();
    }
    let suffix = short_suffix(server_id);
    let first_candidate = format!("{}-{}", base, suffix);
    if !used.contains(&first_candidate) {
        return first_candidate;
    }
    let mut idx = 2;
    loop {
        let candidate = format!("{}-{}-{}", base, suffix, idx);
        if !used.contains(&candidate) {
            return candidate;
        }
        idx += 1;
    }
}

pub(in crate::mcp_servers) fn ensure_server_config_keys(state: &mut MCPServersState) -> bool {
    let mut changed = false;
    let mut used = HashSet::new();

    for server in state.servers.iter_mut() {
        let base = server
            .config_key
            .as_ref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| slugify_server_name(&server.name));
        let unique = unique_config_key(&base, &server.id, &used);
        if server.config_key.as_deref() != Some(unique.as_str()) {
            server.config_key = Some(unique.clone());
            changed = true;
        }
        used.insert(unique);
    }

    changed
}

pub(in crate::mcp_servers) fn comparable_url(server: &MCPServer) -> Option<String> {
    server.http_url.clone().or_else(|| server.url.clone())
}

pub(in crate::mcp_servers) fn server_definition_eq(a: &MCPServer, b: &MCPServer) -> bool {
    a.transport == b.transport
        && a.command == b.command
        && a.args == b.args
        && a.cwd == b.cwd
        && comparable_url(a) == comparable_url(b)
        && a.env == b.env
        && a.headers == b.headers
        && a.timeout == b.timeout
        && a.trust == b.trust
}

pub(in crate::mcp_servers) fn merge_discovered_servers(
    state: &mut MCPServersState,
    local: &LocalModelConfigs,
) -> bool {
    let mut changed = false;
    let mut existing_keys = state
        .servers
        .iter()
        .map(|server| {
            server
                .config_key
                .clone()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| slugify_server_name(&server.name))
        })
        .collect::<HashSet<_>>();

    let mut selected: HashMap<String, (MCPServer, MCPModel)> = HashMap::new();
    let sources = [
        (MCPModel::Claude, &local.claude),
        (MCPModel::Codex, &local.codex),
        (MCPModel::Gemini, &local.gemini),
        (MCPModel::Opencode, &local.opencode),
    ];

    for (model, source) in sources {
        let mut keys = source.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        for key in keys {
            let Some(candidate) = source.get(&key).cloned() else {
                continue;
            };
            if let Some((existing, existing_model)) = selected.get(&key) {
                if !server_definition_eq(existing, &candidate) {
                    log::warn!(
                        "MCP key conflict for '{}': keep {:?}, ignore {:?}",
                        key,
                        existing_model,
                        model
                    );
                }
                continue;
            }
            selected.insert(key, (candidate, model));
        }
    }

    for (key, (candidate, model)) in selected {
        if existing_keys.contains(&key) {
            continue;
        }
        let now = Utc::now();
        let mut discovered = candidate.clone();
        discovered.id = format!("mcp-{}", uuid::Uuid::new_v4());
        discovered.name = if discovered.name.trim().is_empty() {
            display_name_from_key(&key)
        } else {
            discovered.name
        };
        discovered.config_key = Some(key.clone());
        discovered.description = discovered.description.or(Some(format!(
            "Discovered from {} local MCP config",
            match model {
                MCPModel::Claude => "Claude",
                MCPModel::Codex => "Codex",
                MCPModel::Gemini => "Gemini",
                MCPModel::Opencode => "OpenCode",
            }
        )));
        discovered.linked_provider_ids = vec![];
        discovered.created_at = now;
        discovered.updated_at = now;
        state.servers.push(discovered);
        existing_keys.insert(key);
        changed = true;
    }

    changed
}

pub(in crate::mcp_servers) fn load_state_with_local_sync(
) -> Result<(MCPServersState, ModelKeysets), String> {
    let mut state = load_state()?;
    let mut changed = ensure_server_config_keys(&mut state);
    let local = read_local_model_configs();
    if merge_discovered_servers(&mut state, &local) {
        changed = true;
    }
    if ensure_server_config_keys(&mut state) {
        changed = true;
    }
    if changed {
        save_state(&state)?;
    }
    Ok((state, local.keysets()))
}

/// 加载 MCP Servers 状态
pub(in crate::mcp_servers) fn load_state() -> Result<MCPServersState, String> {
    let path = get_mcp_servers_path()?;
    let legacy_path = get_legacy_mcp_servers_path()?;
    let target = if path.exists() {
        path.clone()
    } else {
        legacy_path
    };

    if !target.exists() {
        return Ok(MCPServersState::default());
    }

    let content = fs::read_to_string(&target).map_err(|e| e.to_string())?;
    let mut state: MCPServersState = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    // 如果已加密，解密数据
    if state.is_encrypted {
        for server in state.servers.iter_mut() {
            let _ = decrypt_sensitive_data(server);
        }
    }

    Ok(state)
}

/// 保存 MCP Servers 状态
pub(in crate::mcp_servers) fn save_state(state: &MCPServersState) -> Result<(), String> {
    let path = get_mcp_servers_path()?;

    // 深拷贝并加密
    let mut encrypted_state = state.clone();
    encrypted_state.is_encrypted = true;

    for server in encrypted_state.servers.iter_mut() {
        let _ = encrypt_sensitive_data(server);
    }

    let content = serde_json::to_string_pretty(&encrypted_state).unwrap();
    atomic_write(&path, &content)?;

    let legacy_path = get_legacy_mcp_servers_path()?;
    if legacy_path.exists() {
        let _ = fs::remove_file(legacy_path);
    }

    Ok(())
}

pub(in crate::mcp_servers) fn sync_local_install_state_with_current_servers(
    state: &MCPServersState,
    keysets: &ModelKeysets,
) -> Result<HashMap<String, MCPModelSwitchState>, String> {
    let defaults = derive_switch_states(&state.servers, keysets);
    let local = load_local_install_state()?;
    let normalized = normalize_local_install_state(&state.servers, local, &defaults);
    save_local_install_state(&normalized)?;
    Ok(normalized.model_switches)
}

pub(in crate::mcp_servers) fn refresh_local_install_state_from_cli(
    state: &MCPServersState,
) -> Result<HashMap<String, MCPModelSwitchState>, String> {
    let keysets = model_keysets()?;
    let model_switches = derive_switch_states(&state.servers, &keysets);
    let local = MCPLocalInstallState {
        model_switches: model_switches.clone(),
    };
    save_local_install_state(&local)?;
    Ok(model_switches)
}
