use super::{
    atomic_write, get_claude_mcp_path, get_codex_mcp_path, get_gemini_mcp_path,
    get_opencode_mcp_compat_path, get_opencode_mcp_primary_path, get_workspace_claude_mcp_path,
    get_workspace_codex_mcp_path, get_workspace_gemini_mcp_path, get_workspace_opencode_mcp_path,
    parse_codex_mcp_servers, parse_opencode_json_section, parse_standard_json_section,
    read_json_root, set_json_mcp_entry, slugify_server_name, write_json_root, LocalModelConfigs,
    MCPServer, MCPServerTransport, ModelKeysets,
};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs::{self};
use toml_edit::{self, DocumentMut, Item, Table};

pub(in crate::mcp_servers) fn read_local_model_configs() -> LocalModelConfigs {
    let mut configs = LocalModelConfigs::default();

    if let Ok(path) = get_claude_mcp_path() {
        if let Ok(root) = read_json_root(&path) {
            configs.claude = parse_standard_json_section(&root, "mcpServers");
        } else if path.exists() {
            log::warn!("Failed to parse Claude MCP config: {}", path.display());
        }
    }

    if let Ok(path) = get_codex_mcp_path() {
        if path.exists() {
            match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|content| parse_codex_mcp_servers(&content))
            {
                Ok(entries) => configs.codex = entries,
                Err(err) => log::warn!(
                    "Failed to parse Codex MCP config {}: {}",
                    path.display(),
                    err
                ),
            }
        }
    }

    if let Ok(path) = get_gemini_mcp_path() {
        if let Ok(root) = read_json_root(&path) {
            configs.gemini = parse_standard_json_section(&root, "mcpServers");
        } else if path.exists() {
            log::warn!("Failed to parse Gemini MCP config: {}", path.display());
        }
    }

    let mut opencode_entries = HashMap::new();
    if let Ok(primary_path) = get_opencode_mcp_primary_path() {
        if let Ok(root) = read_json_root(&primary_path) {
            opencode_entries = parse_opencode_json_section(&root);
        } else if primary_path.exists() {
            log::warn!(
                "Failed to parse OpenCode MCP config: {}",
                primary_path.display()
            );
        }
    }
    if let Ok(compat_path) = get_opencode_mcp_compat_path() {
        if compat_path.exists() {
            match read_json_root(&compat_path) {
                Ok(root) => {
                    for (key, server) in parse_opencode_json_section(&root) {
                        opencode_entries.entry(key).or_insert(server);
                    }
                }
                Err(err) => log::warn!(
                    "Failed to parse OpenCode compatibility MCP config {}: {}",
                    compat_path.display(),
                    err
                ),
            }
        }
    }
    configs.opencode = opencode_entries;

    configs
}

pub(in crate::mcp_servers) fn model_keysets() -> Result<ModelKeysets, String> {
    Ok(read_local_model_configs().keysets())
}

pub(in crate::mcp_servers) fn build_standard_entry(
    server: &MCPServer,
    include_type: bool,
) -> Value {
    let mut obj = Map::new();

    if include_type {
        let kind = match server.transport {
            MCPServerTransport::Stdio => "stdio",
            MCPServerTransport::Http => "http",
            MCPServerTransport::Sse => "sse",
        };
        obj.insert("type".to_string(), Value::String(kind.to_string()));
    }

    match server.transport {
        MCPServerTransport::Stdio => {
            if let Some(command) = &server.command {
                obj.insert("command".to_string(), Value::String(command.clone()));
            }
            if let Some(args) = &server.args {
                obj.insert(
                    "args".to_string(),
                    Value::Array(args.iter().map(|arg| Value::String(arg.clone())).collect()),
                );
            }
            if let Some(cwd) = &server.cwd {
                if !cwd.trim().is_empty() {
                    obj.insert("cwd".to_string(), Value::String(cwd.clone()));
                }
            }
        }
        MCPServerTransport::Http | MCPServerTransport::Sse => {
            if let Some(url) = server.http_url.clone().or_else(|| server.url.clone()) {
                obj.insert("url".to_string(), Value::String(url));
            }
        }
    }

    if let Some(env) = &server.env {
        if !env.is_empty() {
            let env_obj = env
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect::<Map<String, Value>>();
            obj.insert("env".to_string(), Value::Object(env_obj));
        }
    }

    if let Some(headers) = &server.headers {
        if !headers.is_empty() {
            let headers_obj = headers
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect::<Map<String, Value>>();
            obj.insert("headers".to_string(), Value::Object(headers_obj));
        }
    }

    if let Some(timeout) = server.timeout {
        obj.insert("timeout".to_string(), Value::Number(timeout.into()));
    }
    if let Some(trust) = server.trust {
        obj.insert("trust".to_string(), Value::Bool(trust));
    }

    Value::Object(obj)
}

pub(in crate::mcp_servers) fn map_to_inline_table(
    map: &HashMap<String, String>,
) -> toml_edit::Value {
    let mut inline = toml_edit::InlineTable::new();
    let mut pairs = map.iter().collect::<Vec<_>>();
    pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (key, value) in pairs {
        inline.insert(key, toml_edit::Value::from(value.clone()));
    }
    toml_edit::Value::InlineTable(inline)
}

pub(in crate::mcp_servers) fn build_codex_entry(server: &MCPServer) -> Table {
    let mut table = Table::new();
    let transport = match server.transport {
        MCPServerTransport::Stdio => "stdio",
        MCPServerTransport::Http => "http",
        MCPServerTransport::Sse => "sse",
    };
    table["type"] = toml_edit::value(transport);

    match server.transport {
        MCPServerTransport::Stdio => {
            if let Some(command) = &server.command {
                table["command"] = toml_edit::value(command.clone());
            }
            if let Some(args) = &server.args {
                let mut arr = toml_edit::Array::new();
                for arg in args {
                    arr.push(arg.clone());
                }
                table["args"] = Item::Value(toml_edit::Value::Array(arr));
            }
            if let Some(cwd) = &server.cwd {
                if !cwd.trim().is_empty() {
                    table["cwd"] = toml_edit::value(cwd.clone());
                }
            }
        }
        MCPServerTransport::Http | MCPServerTransport::Sse => {
            if let Some(url) = server.http_url.clone().or_else(|| server.url.clone()) {
                table["url"] = toml_edit::value(url);
            }
        }
    }

    if let Some(env) = &server.env {
        if !env.is_empty() {
            table["env"] = Item::Value(map_to_inline_table(env));
        }
    }
    if let Some(headers) = &server.headers {
        if !headers.is_empty() {
            table["headers"] = Item::Value(map_to_inline_table(headers));
        }
    }
    if let Some(timeout) = server.timeout {
        table["timeout"] = toml_edit::value(timeout as i64);
    }
    if let Some(trust) = server.trust {
        table["trust"] = toml_edit::value(trust);
    }

    table
}

pub(in crate::mcp_servers) fn build_opencode_entry(server: &MCPServer) -> Value {
    let mut obj = Map::new();
    match server.transport {
        MCPServerTransport::Stdio => {
            obj.insert("type".to_string(), Value::String("local".to_string()));
            if let Some(command) = &server.command {
                let mut cmd = vec![Value::String(command.clone())];
                if let Some(args) = &server.args {
                    cmd.extend(args.iter().map(|arg| Value::String(arg.clone())));
                }
                obj.insert("command".to_string(), Value::Array(cmd));
            }
            if let Some(cwd) = &server.cwd {
                if !cwd.trim().is_empty() {
                    obj.insert("cwd".to_string(), Value::String(cwd.clone()));
                }
            }
        }
        MCPServerTransport::Http | MCPServerTransport::Sse => {
            obj.insert("type".to_string(), Value::String("remote".to_string()));
            if let Some(url) = server.http_url.clone().or_else(|| server.url.clone()) {
                obj.insert("url".to_string(), Value::String(url));
            }
        }
    }

    if let Some(env) = &server.env {
        if !env.is_empty() {
            let env_obj = env
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect::<Map<String, Value>>();
            obj.insert("env".to_string(), Value::Object(env_obj));
        }
    }

    if let Some(headers) = &server.headers {
        if !headers.is_empty() {
            let headers_obj = headers
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect::<Map<String, Value>>();
            obj.insert("headers".to_string(), Value::Object(headers_obj));
        }
    }

    if let Some(timeout) = server.timeout {
        obj.insert("timeout".to_string(), Value::Number(timeout.into()));
    }
    if let Some(trust) = server.trust {
        obj.insert("trust".to_string(), Value::Bool(trust));
    }

    Value::Object(obj)
}

pub(in crate::mcp_servers) fn workspace_managed_key(server: &MCPServer) -> String {
    let base = server
        .config_key
        .clone()
        .unwrap_or_else(|| slugify_server_name(&server.name));
    format!("onespace-{}", base)
}

pub(in crate::mcp_servers) fn clear_workspace_managed_json_entries(
    root: &mut Map<String, Value>,
    section: &str,
) {
    let mut next = root
        .get(section)
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    next.retain(|key, _| !key.starts_with("onespace-"));
    if next.is_empty() {
        root.remove(section);
    } else {
        root.insert(section.to_string(), Value::Object(next));
    }
}

pub(in crate::mcp_servers) fn clear_workspace_managed_codex_entries(doc: &mut DocumentMut) {
    if let Some(table) = doc
        .get_mut("mcp_servers")
        .and_then(|item| item.as_table_mut())
    {
        let keys = table
            .iter()
            .map(|(key, _)| key.to_string())
            .filter(|key| key.starts_with("onespace-"))
            .collect::<Vec<_>>();
        for key in keys {
            table.remove(&key);
        }
        if table.is_empty() {
            doc.remove("mcp_servers");
        }
    }
}

pub(in crate::mcp_servers) fn apply_workspace_claude_servers(
    project_root: &str,
    servers: &[MCPServer],
) -> Result<(), String> {
    let path = get_workspace_claude_mcp_path(project_root);
    let mut root = read_json_root(&path)?;
    clear_workspace_managed_json_entries(&mut root, "mcpServers");
    for server in servers {
        let key = workspace_managed_key(server);
        set_json_mcp_entry(
            &mut root,
            "mcpServers",
            &key,
            Some(build_standard_entry(server, true)),
        );
    }
    write_json_root(&path, &root)
}

pub(in crate::mcp_servers) fn apply_workspace_gemini_servers(
    project_root: &str,
    servers: &[MCPServer],
) -> Result<(), String> {
    let path = get_workspace_gemini_mcp_path(project_root);
    let mut root = read_json_root(&path)?;
    clear_workspace_managed_json_entries(&mut root, "mcpServers");
    for server in servers {
        let key = workspace_managed_key(server);
        set_json_mcp_entry(
            &mut root,
            "mcpServers",
            &key,
            Some(build_standard_entry(server, true)),
        );
    }
    write_json_root(&path, &root)
}

pub(in crate::mcp_servers) fn apply_workspace_codex_servers(
    project_root: &str,
    servers: &[MCPServer],
) -> Result<(), String> {
    let path = get_workspace_codex_mcp_path(project_root);
    let mut doc = if path.exists() {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        content
            .parse::<DocumentMut>()
            .map_err(|e| format!("Invalid TOML {}: {}", path.display(), e))?
    } else {
        DocumentMut::new()
    };

    clear_workspace_managed_codex_entries(&mut doc);
    if !servers.is_empty() {
        if !doc.contains_key("mcp_servers") {
            doc["mcp_servers"] = Item::Table(Table::new());
        }
        if let Some(table) = doc["mcp_servers"].as_table_mut() {
            for server in servers {
                table.insert(
                    workspace_managed_key(server).as_str(),
                    Item::Table(build_codex_entry(server)),
                );
            }
        }
    }

    atomic_write(&path, &doc.to_string())
}

pub(in crate::mcp_servers) fn apply_workspace_opencode_servers(
    project_root: &str,
    servers: &[MCPServer],
) -> Result<(), String> {
    let path = get_workspace_opencode_mcp_path(project_root);
    let mut root = read_json_root(&path)?;
    clear_workspace_managed_json_entries(&mut root, "mcp");
    for server in servers {
        let key = workspace_managed_key(server);
        set_json_mcp_entry(&mut root, "mcp", &key, Some(build_opencode_entry(server)));
    }
    write_json_root(&path, &root)
}

pub(crate) fn apply_project_workspace_servers(
    project_root: &str,
    model: &str,
    servers: &[MCPServer],
) -> Result<(), String> {
    let normalized_root = crate::ai_sessions::normalize_working_dir_for_terminal(project_root);
    if normalized_root.trim().is_empty() {
        return Err("workspace project root is required".to_string());
    }
    match model.trim().to_lowercase().as_str() {
        "claude" => apply_workspace_claude_servers(&normalized_root, servers),
        "codex" => apply_workspace_codex_servers(&normalized_root, servers),
        "gemini" => apply_workspace_gemini_servers(&normalized_root, servers),
        "opencode" => apply_workspace_opencode_servers(&normalized_root, servers),
        _ => Ok(()),
    }
}

pub(in crate::mcp_servers) fn apply_claude_switch(
    server: &MCPServer,
    key: &str,
    enabled: bool,
) -> Result<(), String> {
    let path = get_claude_mcp_path()?;
    let mut root = read_json_root(&path)?;
    let entry = if enabled {
        Some(build_standard_entry(server, true))
    } else {
        None
    };
    set_json_mcp_entry(&mut root, "mcpServers", key, entry);
    write_json_root(&path, &root)
}

pub(in crate::mcp_servers) fn apply_gemini_switch(
    server: &MCPServer,
    key: &str,
    enabled: bool,
) -> Result<(), String> {
    let path = get_gemini_mcp_path()?;
    let mut root = read_json_root(&path)?;
    let entry = if enabled {
        Some(build_standard_entry(server, true))
    } else {
        None
    };
    set_json_mcp_entry(&mut root, "mcpServers", key, entry);
    write_json_root(&path, &root)
}

pub(in crate::mcp_servers) fn apply_codex_switch(
    server: &MCPServer,
    key: &str,
    enabled: bool,
) -> Result<(), String> {
    let path = get_codex_mcp_path()?;
    let mut doc = if path.exists() {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        content
            .parse::<DocumentMut>()
            .map_err(|e| format!("Invalid TOML {}: {}", path.display(), e))?
    } else {
        DocumentMut::new()
    };

    if enabled {
        if !doc.contains_key("mcp_servers") {
            doc["mcp_servers"] = Item::Table(Table::new());
        }
        if let Some(table) = doc["mcp_servers"].as_table_mut() {
            table.insert(key, Item::Table(build_codex_entry(server)));
        }
    } else if let Some(table) = doc.get_mut("mcp_servers").and_then(|v| v.as_table_mut()) {
        table.remove(key);
        if table.is_empty() {
            doc.remove("mcp_servers");
        }
    }

    atomic_write(&path, &doc.to_string())
}

pub(in crate::mcp_servers) fn apply_opencode_switch(
    server: &MCPServer,
    key: &str,
    enabled: bool,
) -> Result<(), String> {
    let primary_path = get_opencode_mcp_primary_path()?;
    let compat_path = get_opencode_mcp_compat_path()?;

    let mut primary_root = read_json_root(&primary_path)?;
    let entry = if enabled {
        Some(build_opencode_entry(server))
    } else {
        None
    };
    set_json_mcp_entry(&mut primary_root, "mcp", key, entry.clone());
    write_json_root(&primary_path, &primary_root)?;

    if compat_path.exists() {
        let mut compat_root = read_json_root(&compat_path)?;
        set_json_mcp_entry(&mut compat_root, "mcp", key, entry);
        write_json_root(&compat_path, &compat_root)?;
    }

    Ok(())
}
