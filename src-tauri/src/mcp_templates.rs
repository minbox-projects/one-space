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
        MCPTemplate {
            id: "everything",
            name: "Everything MCP",
            description: "Reference MCP server that exposes sample tools, prompts, and resources",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-everything"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "debug",
            name: "Debug MCP",
            description: "Debug and inspect MCP interactions during local development",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-debug"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "pdf",
            name: "PDF MCP",
            description: "Parse and process PDF documents for downstream assistant workflows",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-pdf"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "transcript",
            name: "Transcript MCP",
            description: "Extract and analyze transcript-style content in MCP-compatible tools",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-transcript"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "wiki-explorer",
            name: "Wiki Explorer MCP",
            description: "Browse and query wiki-style knowledge sources",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-wiki-explorer"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "system-monitor",
            name: "System Monitor MCP",
            description: "Inspect local machine metrics and runtime diagnostics",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-system-monitor"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "brave-search",
            name: "Brave Search MCP",
            description: "Search the web through Brave Search API",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-brave-search"]),
            url: None,
            env_placeholders: vec!["BRAVE_API_KEY"],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "slack",
            name: "Slack MCP",
            description: "Work with Slack messages, channels, and team context",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-slack"]),
            url: None,
            env_placeholders: vec!["SLACK_BOT_TOKEN"],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "gitlab",
            name: "GitLab MCP",
            description: "Manage GitLab projects, issues, merge requests, and code workflows",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-gitlab"]),
            url: None,
            env_placeholders: vec!["GITLAB_PERSONAL_ACCESS_TOKEN"],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "google-maps",
            name: "Google Maps MCP",
            description: "Use geocoding, places, and routing capabilities from Google Maps",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-google-maps"]),
            url: None,
            env_placeholders: vec!["GOOGLE_MAPS_API_KEY"],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "redis",
            name: "Redis MCP",
            description: "Query and operate Redis data stores from MCP tools",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-redis"]),
            url: None,
            env_placeholders: vec!["REDIS_URL"],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "aws-kb-retrieval",
            name: "AWS KB Retrieval MCP",
            description: "Retrieve knowledge from AWS-backed RAG knowledge bases",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-aws-kb-retrieval"]),
            url: None,
            env_placeholders: vec!["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION"],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "gdrive",
            name: "Google Drive MCP",
            description: "Access Google Drive files and document metadata",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-gdrive"]),
            url: None,
            env_placeholders: vec!["GDRIVE_CREDENTIALS_PATH", "GDRIVE_OAUTH_PATH"],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "everart",
            name: "EverArt MCP",
            description: "Generate and refine images through EverArt integrations",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-everart"]),
            url: None,
            env_placeholders: vec!["EVERART_API_KEY"],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "puppeteer",
            name: "Puppeteer MCP",
            description: "Automate browser workflows and extract web content with Puppeteer",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-puppeteer"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "playwright",
            name: "Playwright MCP",
            description: "Drive browser automation tasks using Playwright MCP",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@playwright/mcp"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "figma",
            name: "Figma MCP",
            description: "Read and operate Figma design context from MCP clients",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "figma-mcp"]),
            url: None,
            env_placeholders: vec!["FIGMA_API_KEY"],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "linear",
            name: "Linear MCP",
            description: "Manage Linear issues, projects, and team workflows",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "linear-mcp-server"]),
            url: None,
            env_placeholders: vec!["LINEAR_API_KEY"],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "octocode",
            name: "Octocode MCP",
            description: "Code intelligence and repository workflows powered by Octocode",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "octocode-mcp"]),
            url: None,
            env_placeholders: vec!["GITHUB_TOKEN"],
            headers_placeholders: vec![],
            default_timeout: Some(120000),
        },
        MCPTemplate {
            id: "weather",
            name: "Weather MCP",
            description: "Fetch weather and forecast data through a lightweight MCP server",
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@h1deya/mcp-server-weather"]),
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
