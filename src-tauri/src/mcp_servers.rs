use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use toml_edit::{self, DocumentMut, Item, Table};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MCPServerTransport {
    Stdio,
    Http,
    Sse,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MCPServer {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_key: Option<String>,
    pub description: Option<String>,

    // 传输方式
    pub transport: MCPServerTransport,

    // Stdio 方式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    // HTTP/SSE 方式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_url: Option<String>,

    // 环境敏感信息（加密存储）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,

    // 高级配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust: Option<bool>,

    // 关联的供应商 ID 列表
    pub linked_provider_ids: Vec<String>,

    // 元数据
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MCPServersState {
    pub servers: Vec<MCPServer>,
    #[serde(default)]
    pub is_encrypted: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct MCPLocalInstallState {
    #[serde(default)]
    pub model_switches: HashMap<String, MCPModelSwitchState>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MCPModel {
    Claude,
    Codex,
    Gemini,
    Opencode,
}

impl FromStr for MCPModel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "gemini" => Ok(Self::Gemini),
            "opencode" => Ok(Self::Opencode),
            _ => Err(format!("Unsupported MCP model: {}", value)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct MCPModelSwitchState {
    pub claude: bool,
    pub codex: bool,
    pub gemini: bool,
    pub opencode: bool,
}

#[derive(Debug, Default)]
struct ModelKeysets {
    claude: HashSet<String>,
    codex: HashSet<String>,
    gemini: HashSet<String>,
    opencode: HashSet<String>,
}

#[derive(Debug, Default, Clone)]
struct LocalModelConfigs {
    claude: HashMap<String, MCPServer>,
    codex: HashMap<String, MCPServer>,
    gemini: HashMap<String, MCPServer>,
    opencode: HashMap<String, MCPServer>,
}

impl LocalModelConfigs {
    fn keysets(&self) -> ModelKeysets {
        ModelKeysets {
            claude: self.claude.keys().cloned().collect(),
            codex: self.codex.keys().cloned().collect(),
            gemini: self.gemini.keys().cloned().collect(),
            opencode: self.opencode.keys().cloned().collect(),
        }
    }
}

fn get_mcp_servers_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let dir = data_dir.join("data").join("mcp");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.json"))
}

fn get_legacy_mcp_servers_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    Ok(data_dir.join("mcp_servers.json"))
}

fn get_claude_mcp_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".claude.json"))
}

fn get_codex_mcp_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".codex").join("config.toml"))
}

fn get_gemini_mcp_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".gemini").join("settings.json"))
}

fn get_opencode_mcp_primary_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".opencode").join("mcp.json"))
}

fn get_opencode_mcp_compat_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".config").join("opencode").join("opencode.json"))
}

fn get_local_install_state_path() -> Result<PathBuf, String> {
    let app_dir = crate::config::get_app_dir()?;
    let dir = app_dir.join("mcp");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("local_install_state.json"))
}

fn trigger_storage_sync(app: tauri::AppHandle, reason: &str) {
    let reason = reason.to_string();
    tauri::async_runtime::spawn(async move {
        let _ = crate::app_store::sync_enqueue(app, reason).await;
    });
}

/// 加密敏感数据
pub fn encrypt_sensitive_data(server: &mut MCPServer) -> Result<(), String> {
    let password = crate::crypto::get_or_init_master_password()?;

    // 加密 env 中的敏感值
    if let Some(ref mut env) = server.env {
        for (_key, value) in env.iter_mut() {
            if !value.is_empty() && !value.starts_with('$') && !value.starts_with("${") {
                *value = crate::crypto::encrypt(value, &password)?;
            }
        }
    }

    // 加密 headers 中的敏感值
    if let Some(ref mut headers) = server.headers {
        for (key, value) in headers.iter_mut() {
            if key.to_lowercase().contains("auth")
                || key.to_lowercase().contains("key")
                || key.to_lowercase().contains("token")
                || key.to_lowercase().contains("secret")
            {
                if !value.is_empty() && !value.starts_with('$') && !value.starts_with("${") {
                    *value = crate::crypto::encrypt(value, &password)?;
                }
            }
        }
    }

    Ok(())
}

/// 解密敏感数据
pub fn decrypt_sensitive_data(server: &mut MCPServer) -> Result<(), String> {
    let password = crate::crypto::get_or_init_master_password()?;

    if let Some(ref mut env) = server.env {
        for (_, value) in env.iter_mut() {
            if !value.is_empty() && !value.starts_with('$') && !value.starts_with("${") {
                if let Ok(decrypted) = crate::crypto::decrypt(value, &password) {
                    *value = decrypted;
                }
            }
        }
    }

    if let Some(ref mut headers) = server.headers {
        for (key, value) in headers.iter_mut() {
            if key.to_lowercase().contains("auth")
                || key.to_lowercase().contains("key")
                || key.to_lowercase().contains("token")
                || key.to_lowercase().contains("secret")
            {
                if !value.is_empty() && !value.starts_with('$') && !value.starts_with("${") {
                    if let Ok(decrypted) = crate::crypto::decrypt(value, &password) {
                        *value = decrypted;
                    }
                }
            }
        }
    }

    Ok(())
}

/// 原子写入文件
fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let temp_path = path.with_extension("tmp");
    let mut file = File::create(&temp_path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);

    fs::rename(&temp_path, path).map_err(|e| e.to_string())?;

    Ok(())
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
    let content =
        serde_json::to_string_pretty(&Value::Object(root.clone())).map_err(|e| e.to_string())?;
    atomic_write(path, &content)
}

fn set_json_mcp_entry(
    root: &mut Map<String, Value>,
    section: &str,
    key: &str,
    entry: Option<Value>,
) {
    let mut section_map = root
        .remove(section)
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    if let Some(item) = entry {
        section_map.insert(key.to_string(), item);
    } else {
        section_map.remove(key);
    }

    if section_map.is_empty() {
        root.remove(section);
    } else {
        root.insert(section.to_string(), Value::Object(section_map));
    }
}

fn display_name_from_key(key: &str) -> String {
    let words = key
        .split(['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                let mut out = String::new();
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
                out
            } else {
                String::new()
            }
        })
        .collect::<Vec<_>>();
    if words.is_empty() {
        key.to_string()
    } else {
        words.join(" ")
    }
}

fn json_string_map(value: Option<&Value>) -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();
    for (key, val) in value.and_then(|v| v.as_object())? {
        if let Some(text) = val.as_str() {
            map.insert(key.clone(), text.to_string());
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

fn discovered_server_with_fields(
    key: &str,
    transport: MCPServerTransport,
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    url: Option<String>,
    env: Option<HashMap<String, String>>,
    headers: Option<HashMap<String, String>>,
    timeout: Option<u32>,
    trust: Option<bool>,
) -> MCPServer {
    let now = Utc::now();
    let cleaned_args = args.and_then(|v| if v.is_empty() { None } else { Some(v) });
    let cleaned_env = env.and_then(|v| if v.is_empty() { None } else { Some(v) });
    let cleaned_headers = headers.and_then(|v| if v.is_empty() { None } else { Some(v) });
    let normalized_url = url.filter(|v| !v.trim().is_empty());
    MCPServer {
        id: String::new(),
        name: display_name_from_key(key),
        config_key: Some(key.to_string()),
        description: Some("Discovered from local CLI MCP config".to_string()),
        transport: transport.clone(),
        command,
        args: cleaned_args,
        cwd: cwd.filter(|v| !v.trim().is_empty()),
        url: normalized_url.clone(),
        http_url: if matches!(transport, MCPServerTransport::Http) {
            normalized_url
        } else {
            None
        },
        env: cleaned_env,
        headers: cleaned_headers,
        timeout,
        trust,
        linked_provider_ids: vec![],
        created_at: now,
        updated_at: now,
    }
}

fn parse_standard_json_entry(key: &str, value: &Value) -> Option<MCPServer> {
    let obj = value.as_object()?;
    let kind = obj
        .get("type")
        .and_then(|v| v.as_str())
        .map(|v| v.to_lowercase());

    let transport = match kind.as_deref() {
        Some("stdio") => MCPServerTransport::Stdio,
        Some("http") => MCPServerTransport::Http,
        Some("sse") => MCPServerTransport::Sse,
        _ if obj.contains_key("command") => MCPServerTransport::Stdio,
        _ if obj.contains_key("url") || obj.contains_key("http_url") => MCPServerTransport::Http,
        _ => return None,
    };

    let command = obj
        .get("command")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let args = obj.get("args").and_then(|v| {
        let values = v
            .as_array()?
            .iter()
            .filter_map(|item| item.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();
        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    });
    let cwd = obj
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let url = obj
        .get("url")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("http_url").and_then(|v| v.as_str()))
        .map(|v| v.to_string());
    let timeout = obj
        .get("timeout")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok());
    let trust = obj.get("trust").and_then(|v| v.as_bool());

    Some(discovered_server_with_fields(
        key,
        transport,
        command,
        args,
        cwd,
        url,
        json_string_map(obj.get("env")),
        json_string_map(obj.get("headers")),
        timeout,
        trust,
    ))
}

fn parse_standard_json_section(
    root: &Map<String, Value>,
    section: &str,
) -> HashMap<String, MCPServer> {
    let mut parsed = HashMap::new();
    if let Some(entries) = root.get(section).and_then(|v| v.as_object()) {
        for (key, value) in entries {
            if let Some(server) = parse_standard_json_entry(key, value) {
                parsed.insert(key.clone(), server);
            }
        }
    }
    parsed
}

fn parse_opencode_json_entry(key: &str, value: &Value) -> Option<MCPServer> {
    let obj = value.as_object()?;
    let kind = obj
        .get("type")
        .and_then(|v| v.as_str())
        .map(|v| v.to_lowercase());

    let url = obj
        .get("url")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let transport = match kind.as_deref() {
        Some("local") | Some("stdio") => MCPServerTransport::Stdio,
        Some("remote") | Some("http") => MCPServerTransport::Http,
        Some("sse") => MCPServerTransport::Sse,
        _ if obj.contains_key("command") => MCPServerTransport::Stdio,
        _ if url.is_some() => MCPServerTransport::Http,
        _ => return None,
    };

    let (command, args) = if matches!(transport, MCPServerTransport::Stdio) {
        if let Some(array) = obj.get("command").and_then(|v| v.as_array()) {
            let parts = array
                .iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>();
            if parts.is_empty() {
                (None, None)
            } else {
                let cmd = Some(parts[0].clone());
                let rest = if parts.len() > 1 {
                    Some(parts[1..].to_vec())
                } else {
                    None
                };
                (cmd, rest)
            }
        } else {
            (
                obj.get("command")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string()),
                obj.get("args").and_then(|v| {
                    let values = v
                        .as_array()?
                        .iter()
                        .filter_map(|item| item.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>();
                    if values.is_empty() {
                        None
                    } else {
                        Some(values)
                    }
                }),
            )
        }
    } else {
        (None, None)
    };

    let cwd = obj
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let timeout = obj
        .get("timeout")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok());
    let trust = obj.get("trust").and_then(|v| v.as_bool());

    Some(discovered_server_with_fields(
        key,
        transport,
        command,
        args,
        cwd,
        url,
        json_string_map(obj.get("env")),
        json_string_map(obj.get("headers")),
        timeout,
        trust,
    ))
}

fn parse_opencode_json_section(root: &Map<String, Value>) -> HashMap<String, MCPServer> {
    let mut parsed = HashMap::new();
    if let Some(entries) = root.get("mcp").and_then(|v| v.as_object()) {
        for (key, value) in entries {
            if let Some(server) = parse_opencode_json_entry(key, value) {
                parsed.insert(key.clone(), server);
            }
        }
    }
    parsed
}

fn parse_toml_string_map(item: Option<&Item>) -> Option<HashMap<String, String>> {
    let mut out = HashMap::new();
    if let Some(table) = item.and_then(|v| v.as_table()) {
        for (key, val) in table.iter() {
            if let Some(text) = val.as_str() {
                out.insert(key.to_string(), text.to_string());
            }
        }
    } else if let Some(inline) = item
        .and_then(|v| v.as_value())
        .and_then(|value| value.as_inline_table())
    {
        for (key, val) in inline.iter() {
            if let Some(text) = val.as_str() {
                out.insert(key.to_string(), text.to_string());
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn parse_codex_entry(key: &str, item: &Item) -> Option<MCPServer> {
    let kind = item
        .get("type")
        .and_then(|v| v.as_str())
        .map(|v| v.to_lowercase());
    let transport = match kind.as_deref() {
        Some("stdio") => MCPServerTransport::Stdio,
        Some("http") => MCPServerTransport::Http,
        Some("sse") => MCPServerTransport::Sse,
        _ if item.get("command").is_some() => MCPServerTransport::Stdio,
        _ if item.get("url").is_some() => MCPServerTransport::Http,
        _ => return None,
    };

    let command = item
        .get("command")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let args = item.get("args").and_then(|v| {
        let values = v
            .as_array()?
            .iter()
            .filter_map(|item| item.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();
        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    });
    let cwd = item
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let url = item
        .get("url")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let timeout = item
        .get("timeout")
        .and_then(|v| v.as_integer())
        .and_then(|v| u32::try_from(v).ok());
    let trust = item.get("trust").and_then(|v| v.as_bool());

    Some(discovered_server_with_fields(
        key,
        transport,
        command,
        args,
        cwd,
        url,
        parse_toml_string_map(item.get("env")),
        parse_toml_string_map(item.get("headers")),
        timeout,
        trust,
    ))
}

fn parse_codex_mcp_servers(content: &str) -> Result<HashMap<String, MCPServer>, String> {
    let doc = content
        .parse::<DocumentMut>()
        .map_err(|e| format!("Invalid TOML: {}", e))?;
    let mut parsed = HashMap::new();
    if let Some(table) = doc.get("mcp_servers").and_then(|v| v.as_table()) {
        for (key, item) in table.iter() {
            if let Some(server) = parse_codex_entry(key, item) {
                parsed.insert(key.to_string(), server);
            }
        }
    }
    Ok(parsed)
}

fn read_local_model_configs() -> LocalModelConfigs {
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

fn model_keysets() -> Result<ModelKeysets, String> {
    Ok(read_local_model_configs().keysets())
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

fn map_to_inline_table(map: &HashMap<String, String>) -> toml_edit::Value {
    let mut inline = toml_edit::InlineTable::new();
    let mut pairs = map.iter().collect::<Vec<_>>();
    pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (key, value) in pairs {
        inline.insert(key, toml_edit::Value::from(value.clone()));
    }
    toml_edit::Value::InlineTable(inline)
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

fn apply_claude_switch(server: &MCPServer, key: &str, enabled: bool) -> Result<(), String> {
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

fn apply_gemini_switch(server: &MCPServer, key: &str, enabled: bool) -> Result<(), String> {
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

fn apply_codex_switch(server: &MCPServer, key: &str, enabled: bool) -> Result<(), String> {
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

fn apply_opencode_switch(server: &MCPServer, key: &str, enabled: bool) -> Result<(), String> {
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

fn apply_model_switch(
    model: MCPModel,
    server: &MCPServer,
    key: &str,
    enabled: bool,
) -> Result<(), String> {
    match model {
        MCPModel::Claude => apply_claude_switch(server, key, enabled),
        MCPModel::Codex => apply_codex_switch(server, key, enabled),
        MCPModel::Gemini => apply_gemini_switch(server, key, enabled),
        MCPModel::Opencode => apply_opencode_switch(server, key, enabled),
    }
}

fn build_model_switch_state(key: &str, keysets: &ModelKeysets) -> MCPModelSwitchState {
    MCPModelSwitchState {
        claude: keysets.claude.contains(key),
        codex: keysets.codex.contains(key),
        gemini: keysets.gemini.contains(key),
        opencode: keysets.opencode.contains(key),
    }
}

fn load_local_install_state() -> Result<MCPLocalInstallState, String> {
    let path = get_local_install_state_path()?;
    if !path.exists() {
        return Ok(MCPLocalInstallState::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Ok(MCPLocalInstallState::default());
    }
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn save_local_install_state(state: &MCPLocalInstallState) -> Result<(), String> {
    let path = get_local_install_state_path()?;
    let content = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    atomic_write(&path, &content)
}

fn derive_switch_states(
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

fn normalize_local_install_state(
    servers: &[MCPServer],
    mut state: MCPLocalInstallState,
    defaults: &HashMap<String, MCPModelSwitchState>,
) -> MCPLocalInstallState {
    let server_ids = servers
        .iter()
        .map(|s| s.id.clone())
        .collect::<HashSet<_>>();
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

fn slugify_server_name(name: &str) -> String {
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

fn short_suffix(id: &str) -> String {
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

fn unique_config_key(base: &str, server_id: &str, used: &HashSet<String>) -> String {
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

fn ensure_server_config_keys(state: &mut MCPServersState) -> bool {
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

fn comparable_url(server: &MCPServer) -> Option<String> {
    server.http_url.clone().or_else(|| server.url.clone())
}

fn server_definition_eq(a: &MCPServer, b: &MCPServer) -> bool {
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

fn merge_discovered_servers(state: &mut MCPServersState, local: &LocalModelConfigs) -> bool {
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

fn load_state_with_local_sync() -> Result<(MCPServersState, ModelKeysets), String> {
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
fn load_state() -> Result<MCPServersState, String> {
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
fn save_state(state: &MCPServersState) -> Result<(), String> {
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

fn sync_local_install_state_with_current_servers(
    state: &MCPServersState,
    keysets: &ModelKeysets,
) -> Result<HashMap<String, MCPModelSwitchState>, String> {
    let defaults = derive_switch_states(&state.servers, keysets);
    let local = load_local_install_state()?;
    let normalized = normalize_local_install_state(&state.servers, local, &defaults);
    save_local_install_state(&normalized)?;
    Ok(normalized.model_switches)
}

fn refresh_local_install_state_from_cli(
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

/// 获取所有 MCP 服务器
#[tauri::command]
pub fn get_mcp_servers() -> Result<MCPServersState, String> {
    let (state, keysets) = load_state_with_local_sync()?;
    let _ = sync_local_install_state_with_current_servers(&state, &keysets);
    Ok(state)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_server(name: &str) -> MCPServer {
        MCPServer {
            id: format!("mcp-{}", name),
            name: name.to_string(),
            config_key: None,
            description: None,
            transport: MCPServerTransport::Stdio,
            command: Some("npx".to_string()),
            args: Some(vec!["-y".to_string(), "@upstash/context7-mcp".to_string()]),
            cwd: None,
            url: None,
            http_url: None,
            env: None,
            headers: None,
            timeout: Some(60000),
            trust: Some(false),
            linked_provider_ids: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn slugify_and_unique_config_key() {
        let mut state = MCPServersState {
            servers: vec![
                sample_server("Context 7"),
                sample_server("Context-7"),
                sample_server(""),
            ],
            is_encrypted: false,
        };

        assert!(ensure_server_config_keys(&mut state));
        let keys = state
            .servers
            .iter()
            .map(|s| s.config_key.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(keys[0], "context-7");
        assert_ne!(keys[1], "context-7");
        assert!(keys[1].starts_with("context-7-"));
        assert_eq!(keys[2], "server");
    }

    #[test]
    fn standard_entry_contains_transport_and_fields() {
        let server = MCPServer {
            transport: MCPServerTransport::Sse,
            url: Some("http://localhost:3000/sse".to_string()),
            ..sample_server("test")
        };
        let value = build_standard_entry(&server, true);
        let obj = value.as_object().expect("object");
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("sse"));
        assert_eq!(
            obj.get("url").and_then(|v| v.as_str()),
            Some("http://localhost:3000/sse")
        );
    }

    #[test]
    fn codex_entry_serializes_stdio_command() {
        let server = sample_server("codex");
        let table = build_codex_entry(&server);
        assert_eq!(table.get("type").and_then(|v| v.as_str()), Some("stdio"));
        assert_eq!(table.get("command").and_then(|v| v.as_str()), Some("npx"));
        assert!(table.get("args").is_some());
    }

    #[test]
    fn remove_json_entry_keeps_others() {
        let mut root = Map::new();
        root.insert(
            "mcpServers".to_string(),
            serde_json::json!({
                "keep": { "type": "stdio", "command": "npx" },
                "drop": { "type": "stdio", "command": "uvx" }
            }),
        );
        set_json_mcp_entry(&mut root, "mcpServers", "drop", None);
        let section = root
            .get("mcpServers")
            .and_then(|v| v.as_object())
            .expect("section exists");
        assert!(section.contains_key("keep"));
        assert!(!section.contains_key("drop"));
    }

    #[test]
    fn opencode_entry_maps_to_local_and_remote() {
        let local = build_opencode_entry(&sample_server("local"));
        assert_eq!(
            local
                .as_object()
                .and_then(|obj| obj.get("type"))
                .and_then(|v| v.as_str()),
            Some("local")
        );

        let remote_server = MCPServer {
            transport: MCPServerTransport::Http,
            http_url: Some("https://example.com/mcp".to_string()),
            ..sample_server("remote")
        };
        let remote = build_opencode_entry(&remote_server);
        let obj = remote.as_object().expect("remote object");
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("remote"));
        assert_eq!(
            obj.get("url").and_then(|v| v.as_str()),
            Some("https://example.com/mcp")
        );
    }

    #[test]
    fn parse_codex_mcp_servers_supports_stdio_and_remote() {
        let content = r#"
[mcp_servers.context7]
type = "stdio"
command = "npx"
args = ["-y", "@upstash/context7-mcp"]
timeout = 30000

[mcp_servers.remoteapi]
type = "http"
url = "https://example.com/mcp"
trust = true
"#;
        let parsed = parse_codex_mcp_servers(content).expect("parsed");
        let stdio = parsed.get("context7").expect("stdio entry");
        assert_eq!(stdio.transport, MCPServerTransport::Stdio);
        assert_eq!(stdio.command.as_deref(), Some("npx"));
        assert_eq!(stdio.timeout, Some(30000));

        let remote = parsed.get("remoteapi").expect("remote entry");
        assert_eq!(remote.transport, MCPServerTransport::Http);
        assert_eq!(remote.http_url.as_deref(), Some("https://example.com/mcp"));
        assert_eq!(remote.trust, Some(true));
    }

    #[test]
    fn parse_opencode_json_section_supports_local_and_remote() {
        let mut root = Map::new();
        root.insert(
            "mcp".to_string(),
            serde_json::json!({
                "local_one": {
                    "type": "local",
                    "command": ["uvx", "mcp-local"],
                    "cwd": "/tmp/local"
                },
                "remote_one": {
                    "type": "remote",
                    "url": "https://example.com/remote"
                }
            }),
        );
        let parsed = parse_opencode_json_section(&root);
        let local = parsed.get("local_one").expect("local entry");
        assert_eq!(local.transport, MCPServerTransport::Stdio);
        assert_eq!(local.command.as_deref(), Some("uvx"));
        assert_eq!(
            local
                .args
                .as_ref()
                .and_then(|v| v.first())
                .map(|s| s.as_str()),
            Some("mcp-local")
        );
        assert_eq!(local.cwd.as_deref(), Some("/tmp/local"));

        let remote = parsed.get("remote_one").expect("remote entry");
        assert_eq!(remote.transport, MCPServerTransport::Http);
        assert_eq!(
            remote.http_url.as_deref(),
            Some("https://example.com/remote")
        );
    }

    #[test]
    fn merge_discovered_servers_keeps_existing_onespace_definition() {
        let mut existing = sample_server("existing");
        existing.config_key = Some("shared".to_string());
        existing.command = Some("onespace-command".to_string());

        let mut discovered = sample_server("discovered");
        discovered.config_key = Some("shared".to_string());
        discovered.command = Some("external-command".to_string());

        let mut state = MCPServersState {
            servers: vec![existing.clone()],
            is_encrypted: false,
        };
        let mut local = LocalModelConfigs::default();
        local.claude.insert("shared".to_string(), discovered);

        let changed = merge_discovered_servers(&mut state, &local);
        assert!(!changed);
        assert_eq!(state.servers.len(), 1);
        assert_eq!(
            state.servers[0].command.as_deref(),
            Some("onespace-command")
        );
    }

    #[test]
    fn merge_discovered_servers_prefers_priority_when_conflict() {
        let mut claude = sample_server("claude-candidate");
        claude.config_key = Some("same-key".to_string());
        claude.command = Some("claude-command".to_string());

        let mut codex = sample_server("codex-candidate");
        codex.config_key = Some("same-key".to_string());
        codex.command = Some("codex-command".to_string());

        let mut local = LocalModelConfigs::default();
        local.claude.insert("same-key".to_string(), claude);
        local.codex.insert("same-key".to_string(), codex);

        let mut state = MCPServersState::default();
        let changed = merge_discovered_servers(&mut state, &local);
        assert!(changed);
        assert_eq!(state.servers.len(), 1);
        assert_eq!(state.servers[0].config_key.as_deref(), Some("same-key"));
        assert_eq!(state.servers[0].command.as_deref(), Some("claude-command"));
    }

    #[test]
    fn build_model_switch_state_marks_multiple_models() {
        let mut keysets = ModelKeysets::default();
        keysets.claude.insert("alpha".to_string());
        keysets.codex.insert("alpha".to_string());
        keysets.gemini.insert("beta".to_string());
        let state = build_model_switch_state("alpha", &keysets);
        assert!(state.claude);
        assert!(state.codex);
        assert!(!state.gemini);
        assert!(!state.opencode);
    }
}
