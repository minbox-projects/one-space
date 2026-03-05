use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;

#[derive(Debug, Serialize, Deserialize)]
pub struct MCPExportConfig {
    pub export_version: String,
    pub exported_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exported_by: Option<String>,
    pub servers: Vec<MCPExportServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MCPExportServer {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub transport: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_url: Option<String>,
    #[serde(default)]
    pub env_placeholders: Vec<String>,
    #[serde(default)]
    pub headers_placeholders: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust: Option<bool>,
}

#[tauri::command]
pub fn export_mcp_config(
    server_ids: Vec<String>,
    output_path: String,
    notes: Option<String>,
) -> Result<String, String> {
    let state = crate::mcp_servers::get_mcp_servers().map_err(|e| e.to_string())?;
    let mut export_servers = vec![];

    for server in state.servers.iter() {
        if !server_ids.is_empty() && !server_ids.contains(&server.id) {
            continue;
        }

        let mut env_placeholders = vec![];
        if let Some(ref env) = server.env {
            for (key, value) in env {
                if value.starts_with('$') || value.starts_with("${") {
                    env_placeholders.push(key.clone());
                }
            }
        }

        let mut headers_placeholders = vec![];
        if let Some(ref headers) = server.headers {
            for key in headers.keys() {
                if key.to_lowercase().contains("auth")
                    || key.to_lowercase().contains("key")
                    || key.to_lowercase().contains("token")
                    || key.to_lowercase().contains("secret")
                {
                    headers_placeholders.push(key.clone());
                }
            }
        }

        export_servers.push(MCPExportServer {
            name: server.name.clone(),
            description: server.description.clone(),
            transport: format!("{:?}", server.transport).to_lowercase(),
            command: server.command.clone(),
            args: server.args.clone(),
            url: server.url.clone(),
            http_url: server.http_url.clone(),
            env_placeholders,
            headers_placeholders,
            timeout: server.timeout,
            trust: server.trust,
        });
    }

    let export_config = MCPExportConfig {
        export_version: "1.0".to_string(),
        exported_at: chrono::Utc::now().to_rfc3339(),
        exported_by: None,
        servers: export_servers,
        notes,
    };

    let content = serde_json::to_string_pretty(&export_config).map_err(|e| e.to_string())?;

    if let Some(parent) = std::path::Path::new(&output_path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut file = File::create(&output_path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes())
        .map_err(|e| e.to_string())?;

    Ok(output_path)
}

#[tauri::command]
pub fn import_mcp_config(
    import_path: String,
    link_to_provider_ids: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    use chrono::Utc;

    let content = fs::read_to_string(&import_path).map_err(|e| e.to_string())?;
    let export_config: MCPExportConfig =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let mut imported_ids = vec![];
    let now = Utc::now();

    for server_export in export_config.servers {
        let mut env: Option<HashMap<String, String>> = None;
        if !server_export.env_placeholders.is_empty() {
            let mut env_map: HashMap<String, String> = HashMap::new();
            for placeholder in &server_export.env_placeholders {
                env_map.insert(placeholder.clone(), format!("${}", placeholder));
            }
            env = Some(env_map);
        }

        let mut headers: Option<HashMap<String, String>> = None;
        if !server_export.headers_placeholders.is_empty() {
            let mut headers_map: HashMap<String, String> = HashMap::new();
            for placeholder in &server_export.headers_placeholders {
                headers_map.insert(placeholder.clone(), "${}".to_string());
            }
            headers = Some(headers_map);
        }

        let transport = match server_export.transport.as_str() {
            "stdio" => crate::mcp_servers::MCPServerTransport::Stdio,
            "http" => crate::mcp_servers::MCPServerTransport::Http,
            "sse" => crate::mcp_servers::MCPServerTransport::Sse,
            _ => crate::mcp_servers::MCPServerTransport::Stdio,
        };

        let new_server = crate::mcp_servers::MCPServer {
            id: format!("mcp-{}", uuid::Uuid::new_v4()),
            name: server_export.name,
            config_key: None,
            description: server_export.description,
            transport,
            command: server_export.command,
            args: server_export.args,
            cwd: None,
            url: server_export.url,
            http_url: server_export.http_url,
            env,
            headers,
            timeout: server_export.timeout,
            trust: server_export.trust,
            linked_provider_ids: link_to_provider_ids.clone().unwrap_or_default(),
            created_at: now,
            updated_at: now,
        };

        crate::mcp_servers::save_mcp_server(new_server.clone()).map_err(|e| e.to_string())?;
        imported_ids.push(new_server.id);
    }

    Ok(imported_ids)
}
