use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

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

fn get_mcp_servers_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    Ok(data_dir.join("mcp_servers.json"))
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

/// 加载 MCP Servers 状态
fn load_state() -> Result<MCPServersState, String> {
    let path = get_mcp_servers_path()?;

    if !path.exists() {
        return Ok(MCPServersState::default());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
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

    Ok(())
}

/// 获取所有 MCP 服务器
#[tauri::command]
pub fn get_mcp_servers() -> Result<MCPServersState, String> {
    load_state()
}

/// 保存 MCP 服务器（新增或更新）
#[tauri::command]
pub fn save_mcp_server(server: MCPServer) -> Result<(), String> {
    let mut state = load_state()?;
    let now = Utc::now();

    if let Some(existing) = state.servers.iter_mut().find(|s| s.id == server.id) {
        // 更新现有服务器
        let mut updated_server = server.clone();
        updated_server.created_at = existing.created_at;
        updated_server.updated_at = now;
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

    save_state(&state)?;

    Ok(())
}

/// 删除 MCP 服务器
#[tauri::command]
pub fn delete_mcp_server(server_id: String) -> Result<(), String> {
    let mut state = load_state()?;
    state.servers.retain(|s| s.id != server_id);
    save_state(&state)?;

    Ok(())
}

/// 关联 MCP 服务器到供应商
#[tauri::command]
pub fn link_mcp_to_providers(server_id: String, provider_ids: Vec<String>) -> Result<(), String> {
    let mut state = load_state()?;

    if let Some(server) = state.servers.iter_mut().find(|s| s.id == server_id) {
        server.linked_provider_ids = provider_ids;
        server.updated_at = Utc::now();
        save_state(&state)?;
    } else {
        return Err("MCP Server not found".to_string());
    }

    Ok(())
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
