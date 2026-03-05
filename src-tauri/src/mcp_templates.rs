use crate::mcp_servers::{MCPServer, MCPServerTransport};
use chrono::Utc;
use std::collections::HashMap;

pub struct MCPTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub transport: MCPServerTransport,
    pub command: Option<&'static str>,
    pub args: Option<Vec<&'static str>>,
    pub url: Option<&'static str>,
    pub env_placeholders: Vec<&'static str>,
    pub headers_placeholders: Vec<&'static str>,
    pub default_timeout: Option<u32>,
}

/// 获取所有 MCP 模板
pub fn get_mcp_templates() -> Vec<MCPTemplate> {
    vec![
        MCPTemplate {
            id: "github",
            name: "GitHub MCP",
            description: "Manage GitHub repositories, issues, pull requests, and code reviews",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-github"]),
            url: None,
            env_placeholders: vec!["GITHUB_TOKEN"],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "filesystem",
            name: "Filesystem MCP",
            description: "Read and write files in specified directories with permission control",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-filesystem"]),
            url: None,
            env_placeholders: vec![], // 不需要 API key，通过命令行参数指定允许的目录
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "postgres",
            name: "PostgreSQL MCP",
            description: "Query and manage PostgreSQL databases with SQL execution capabilities",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-postgres"]),
            url: None,
            env_placeholders: vec!["DATABASE_URL"],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "context7",
            name: "Context7 (Upstash)",
            description: "Access documentation and code examples from Upstash Context7",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@upstash/context7-mcp"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "braingrid",
            name: "BrainGrid Remote",
            description: "Remote MCP service for AI workflows and integrations",
            transport: MCPServerTransport::Http,
            command: None,
            args: None,
            url: Some("https://mcp.braingrid.ai/mcp"),
            env_placeholders: vec![],
            headers_placeholders: vec!["Authorization"],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "memory",
            name: "Memory MCP",
            description: "Long-term memory storage and retrieval for AI assistants",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-memory"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "sequential-thinking",
            name: "Sequential Thinking MCP",
            description: "Advanced reasoning through sequential thought processes",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec![
                "-y",
                "@modelcontextprotocol/server-sequential-thinking",
            ]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
    ]
}

/// 从模板创建 MCP 服务器实例
#[tauri::command]
pub fn get_mcp_template(template_id: String) -> Result<MCPServer, String> {
    let templates = get_mcp_templates();
    let template = templates
        .iter()
        .find(|t| t.id == template_id)
        .ok_or("Template not found")?;

    let now = Utc::now();

    // 构建环境变量
    let mut env: Option<HashMap<String, String>> = None;
    if !template.env_placeholders.is_empty() {
        let mut env_map = HashMap::new();
        for placeholder in &template.env_placeholders {
            // 使用占位符格式
            env_map.insert(placeholder.to_string(), format!("${}", placeholder));
        }
        env = Some(env_map);
    }

    // 构建 Headers
    let mut headers: Option<HashMap<String, String>> = None;
    if !template.headers_placeholders.is_empty() {
        let mut headers_map = HashMap::new();
        for placeholder in &template.headers_placeholders {
            headers_map.insert(placeholder.to_string(), "${}".to_string());
        }
        headers = Some(headers_map);
    }

    Ok(MCPServer {
        id: format!("mcp-{}", template.id),
        name: template.name.to_string(),
        config_key: None,
        description: Some(template.description.to_string()),
        transport: template.transport.clone(),
        command: template.command.map(String::from),
        args: template
            .args
            .as_ref()
            .map(|args| args.iter().map(|s| s.to_string()).collect()),
        cwd: None,
        url: template.url.map(String::from),
        http_url: None,
        env,
        headers,
        timeout: template.default_timeout,
        trust: Some(false),
        linked_provider_ids: vec![],
        created_at: now,
        updated_at: now,
    })
}

/// 获取模板列表（用于前端展示）
#[tauri::command]
pub fn list_mcp_templates() -> Result<Vec<serde_json::Value>, String> {
    let templates = get_mcp_templates();

    Ok(templates
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "name": t.name,
                "description": t.description,
                "transport": format!("{:?}", t.transport).to_lowercase(),
                "command": t.command,
                "args": t.args,
                "url": t.url,
                "env_placeholders": t.env_placeholders,
                "headers_placeholders": t.headers_placeholders,
            })
        })
        .collect())
}
