use crate::mcp_servers::{MCPServer, MCPServerTransport};
use crate::get_data_dir;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use toml_edit::{self, DocumentMut, Item, Table};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub const DEFAULT_PROFILE_TTL_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct StrictProfileInput {
    pub profile_id: String,
    pub tool: String,
    pub mcp_servers: Vec<MCPServer>,
    pub skill_dir_names: Vec<String>,
    pub reuse_existing: bool,
}

#[derive(Debug, Clone)]
pub struct StrictProfileResult {
    pub profile_id: String,
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sanitize_profile_id(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        }
    }
    if out.is_empty() {
        format!("rp-{}", now_ts())
    } else {
        out
    }
}

pub fn runtime_profiles_root() -> Result<PathBuf, String> {
    let root = get_data_dir()?.join("data").join("runtime_profiles");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
}

pub fn runtime_profile_dir(profile_id: &str) -> Result<PathBuf, String> {
    Ok(runtime_profiles_root()?.join(sanitize_profile_id(profile_id)))
}

pub fn runtime_profile_exists(profile_id: &str) -> Result<bool, String> {
    Ok(runtime_profile_dir(profile_id)?.exists())
}

fn set_dir_mode_700(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn set_file_mode_600(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    set_dir_mode_700(path)
}

fn read_json_root(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(Map::new());
    }
    let value: Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid JSON {}: {}", path.display(), e))?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| format!("Expected JSON object in {}", path.display()))
}

fn write_json_root(path: &Path, root: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let content =
        serde_json::to_string_pretty(&Value::Object(root.clone())).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())?;
    set_file_mode_600(path)
}

fn set_json_section_map(root: &mut Map<String, Value>, section: &str, entries: Map<String, Value>) {
    if entries.is_empty() {
        root.remove(section);
    } else {
        root.insert(section.to_string(), Value::Object(entries));
    }
}

fn map_to_inline_table(map: &HashMap<String, String>) -> toml_edit::Value {
    let mut inline = toml_edit::InlineTable::new();
    let mut pairs = map.iter().collect::<Vec<_>>();
    pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (key, value) in pairs {
        inline.insert(key, toml_edit::Value::from(value.clone()));
    }
    toml_edit::Value::InlineTable(inline)
}

fn build_standard_entry(server: &MCPServer, include_type: bool) -> Value {
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

fn build_codex_entry(server: &MCPServer) -> Table {
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

fn build_opencode_entry(server: &MCPServer) -> Value {
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

fn server_key(server: &MCPServer) -> String {
    server
        .config_key
        .clone()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| server.id.clone())
}

fn selected_entries_json(servers: &[MCPServer], include_type: bool) -> Map<String, Value> {
    let mut out = Map::new();
    let mut sorted = servers.to_vec();
    sorted.sort_by(|a, b| server_key(a).cmp(&server_key(b)));
    for server in sorted {
        out.insert(server_key(&server), build_standard_entry(&server, include_type));
    }
    out
}

fn selected_entries_opencode(servers: &[MCPServer]) -> Map<String, Value> {
    let mut out = Map::new();
    let mut sorted = servers.to_vec();
    sorted.sort_by(|a, b| server_key(a).cmp(&server_key(b)));
    for server in sorted {
        out.insert(server_key(&server), build_opencode_entry(&server));
    }
    out
}

fn render_mcp_for_tool(
    tool: &str,
    home_dir: &Path,
    xdg_config_home: &Path,
    servers: &[MCPServer],
) -> Result<(), String> {
    match tool {
        "claude" => {
            let path = home_dir.join(".claude.json");
            let mut root = read_json_root(&path).unwrap_or_default();
            set_json_section_map(&mut root, "mcpServers", selected_entries_json(servers, true));
            write_json_root(&path, &root)
        }
        "gemini" => {
            let path = home_dir.join(".gemini").join("settings.json");
            let mut root = read_json_root(&path).unwrap_or_default();
            set_json_section_map(&mut root, "mcpServers", selected_entries_json(servers, true));
            write_json_root(&path, &root)
        }
        "codex" => {
            let path = home_dir.join(".codex").join("config.toml");
            if let Some(parent) = path.parent() {
                ensure_dir(parent)?;
            }
            let mut doc = if path.exists() {
                fs::read_to_string(&path)
                    .ok()
                    .and_then(|content| content.parse::<DocumentMut>().ok())
                    .unwrap_or_default()
            } else {
                DocumentMut::new()
            };
            doc.remove("mcp_servers");
            let mut servers_table = Table::new();
            let mut sorted = servers.to_vec();
            sorted.sort_by(|a, b| server_key(a).cmp(&server_key(b)));
            for server in sorted {
                let key = server_key(&server);
                servers_table.insert(&key, Item::Table(build_codex_entry(&server)));
            }
            doc["mcp_servers"] = Item::Table(servers_table);
            fs::write(&path, doc.to_string()).map_err(|e| e.to_string())?;
            set_file_mode_600(&path)
        }
        "opencode" => {
            let primary_path = home_dir.join(".opencode").join("mcp.json");
            let compat_path = xdg_config_home.join("opencode").join("opencode.json");

            let mut primary_root = read_json_root(&primary_path).unwrap_or_default();
            set_json_section_map(&mut primary_root, "mcp", selected_entries_opencode(servers));
            write_json_root(&primary_path, &primary_root)?;

            let mut compat_root = read_json_root(&compat_path).unwrap_or_default();
            set_json_section_map(&mut compat_root, "mcp", selected_entries_opencode(servers));
            write_json_root(&compat_path, &compat_root)
        }
        _ => Err(format!("unsupported tool for strict profile: {}", tool)),
    }
}

fn copy_file_if_exists(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() || !src.is_file() {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        ensure_dir(parent)?;
    }
    fs::copy(src, dst).map_err(|e| e.to_string())?;
    set_file_mode_600(dst)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() || !src.is_dir() {
        return Ok(());
    }
    ensure_dir(dst)?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let meta = fs::symlink_metadata(&src_path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if meta.is_file() {
            if let Some(parent) = dst_path.parent() {
                ensure_dir(parent)?;
            }
            fs::copy(&src_path, &dst_path).map_err(|e| e.to_string())?;
            set_file_mode_600(&dst_path)?;
        }
    }
    Ok(())
}

fn harden_permissions_recursive(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        set_dir_mode_700(path)?;
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            harden_permissions_recursive(&entry.path())?;
        }
    } else if meta.is_file() {
        set_file_mode_600(path)?;
    }
    Ok(())
}

fn profile_tool_skills_dir(tool: &str, home_dir: &Path, xdg_config_home: &Path) -> Result<PathBuf, String> {
    match tool {
        "claude" => Ok(home_dir.join(".claude").join("skills")),
        "codex" => Ok(home_dir.join(".codex").join("skills")),
        "gemini" => Ok(home_dir.join(".gemini").join("skills")),
        "opencode" => Ok(xdg_config_home.join("opencode").join("skills")),
        _ => Err(format!("unsupported tool for skills: {}", tool)),
    }
}

fn global_tool_skills_dir(tool: &str, home: &Path) -> Result<PathBuf, String> {
    match tool {
        "claude" => Ok(home.join(".claude").join("skills")),
        "codex" => Ok(home.join(".codex").join("skills")),
        "gemini" => Ok(home.join(".gemini").join("skills")),
        "opencode" => Ok(home.join(".config").join("opencode").join("skills")),
        _ => Err(format!("unsupported tool for global skills: {}", tool)),
    }
}

fn sync_skills_for_profile(
    tool: &str,
    global_home: &Path,
    home_dir: &Path,
    xdg_config_home: &Path,
    skill_dir_names: &[String],
) -> Result<(), String> {
    let source_root = global_tool_skills_dir(tool, global_home)?;
    let target_root = profile_tool_skills_dir(tool, home_dir, xdg_config_home)?;

    if target_root.exists() {
        fs::remove_dir_all(&target_root).map_err(|e| e.to_string())?;
    }
    ensure_dir(&target_root)?;

    let mut missing = Vec::new();
    for raw_name in skill_dir_names {
        let name = raw_name.trim();
        if name.is_empty() {
            continue;
        }
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(format!("invalid skill dir name: {}", name));
        }
        let src = source_root.join(name);
        let dst = target_root.join(name);
        if !src.exists() {
            missing.push(name.to_string());
            continue;
        }
        copy_dir_recursive(&src, &dst)?;
    }

    if !missing.is_empty() {
        return Err(format!(
            "strict profile skills missing in global mirror: {}",
            missing.join(", ")
        ));
    }

    Ok(())
}

fn copy_tool_baseline(tool: &str, global_home: &Path, home_dir: &Path, xdg_config_home: &Path) -> Result<(), String> {
    match tool {
        "claude" => {
            copy_file_if_exists(&global_home.join(".claude.json"), &home_dir.join(".claude.json"))?;
            copy_dir_recursive(&global_home.join(".claude"), &home_dir.join(".claude"))?;
            Ok(())
        }
        "codex" => copy_dir_recursive(&global_home.join(".codex"), &home_dir.join(".codex")),
        "gemini" => copy_dir_recursive(&global_home.join(".gemini"), &home_dir.join(".gemini")),
        "opencode" => {
            copy_dir_recursive(&global_home.join(".opencode"), &home_dir.join(".opencode"))?;
            copy_dir_recursive(
                &global_home.join(".config").join("opencode"),
                &xdg_config_home.join("opencode"),
            )?;
            Ok(())
        }
        _ => Err(format!("unsupported tool for baseline copy: {}", tool)),
    }
}

pub fn materialize_strict_profile(input: StrictProfileInput) -> Result<StrictProfileResult, String> {
    let tool = input.tool.trim().to_lowercase();
    let profile_id = sanitize_profile_id(&input.profile_id);
    let profile_dir = runtime_profile_dir(&profile_id)?;
    let home_dir = profile_dir.join("home");
    let xdg_config_home = profile_dir.join("xdg_config");
    let xdg_data_home = profile_dir.join("xdg_data");

    if profile_dir.exists() && !input.reuse_existing {
        fs::remove_dir_all(&profile_dir).map_err(|e| e.to_string())?;
    }

    ensure_dir(&profile_dir)?;
    ensure_dir(&home_dir)?;
    ensure_dir(&xdg_config_home)?;
    ensure_dir(&xdg_data_home)?;

    let global_home = dirs::home_dir().ok_or("Could not find home directory")?;

    copy_tool_baseline(&tool, &global_home, &home_dir, &xdg_config_home)?;
    render_mcp_for_tool(&tool, &home_dir, &xdg_config_home, &input.mcp_servers)?;
    sync_skills_for_profile(
        &tool,
        &global_home,
        &home_dir,
        &xdg_config_home,
        &input.skill_dir_names,
    )?;

    let marker = profile_dir.join(".profile_meta.json");
    let meta = serde_json::json!({
        "profile_id": profile_id,
        "tool": tool,
        "updated_at": now_ts(),
    });
    fs::write(&marker, serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    set_file_mode_600(&marker)?;

    harden_permissions_recursive(&profile_dir)?;

    Ok(StrictProfileResult {
        profile_id,
    })
}

pub fn runtime_env_for_profile(profile_id: &str) -> Result<HashMap<String, String>, String> {
    let profile_dir = runtime_profile_dir(profile_id)?;
    let home_dir = profile_dir.join("home");
    let xdg_config_home = profile_dir.join("xdg_config");
    let xdg_data_home = profile_dir.join("xdg_data");

    if !profile_dir.exists() {
        return Err(format!("runtime profile not found: {}", profile_id));
    }

    ensure_dir(&home_dir)?;
    ensure_dir(&xdg_config_home)?;
    ensure_dir(&xdg_data_home)?;

    let mut env = HashMap::new();
    env.insert("HOME".to_string(), home_dir.to_string_lossy().to_string());
    env.insert(
        "XDG_CONFIG_HOME".to_string(),
        xdg_config_home.to_string_lossy().to_string(),
    );
    env.insert(
        "XDG_DATA_HOME".to_string(),
        xdg_data_home.to_string_lossy().to_string(),
    );

    let touch = profile_dir.join(".last_used");
    fs::write(&touch, now_ts().to_string()).map_err(|e| e.to_string())?;
    set_file_mode_600(&touch)?;

    Ok(env)
}

pub fn cleanup_stale_runtime_profiles(
    protected_profile_ids: &HashSet<String>,
    ttl_secs: u64,
) -> Result<Vec<String>, String> {
    let root = runtime_profiles_root()?;
    let now = SystemTime::now();
    let mut removed = Vec::new();

    let entries = fs::read_dir(&root).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if protected_profile_ids.contains(&name) {
            continue;
        }

        let modified = fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let age_secs = now
            .duration_since(modified)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if age_secs < ttl_secs {
            continue;
        }

        if fs::remove_dir_all(&path).is_ok() {
            removed.push(name);
        }
    }

    Ok(removed)
}
