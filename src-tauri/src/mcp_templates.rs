use crate::mcp_servers::{MCPServer, MCPServerTransport};
use chrono::Utc;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct MCPTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub capability_summary: &'static str,
    pub capability_tags: Vec<&'static str>,
    pub impact_tags: Vec<&'static str>,
    pub impact_note: Option<&'static str>,
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
            id: "exa",
            name: "Exa MCP",
            description: "Search the live web through Exa's hosted MCP endpoint",
            category: "search",
            capability_summary: "联网搜索当前网页与新闻结果，适合补充实时来源。",
            capability_tags: vec!["web_search", "current_info", "sources"],
            impact_tags: vec!["network", "remote_api"],
            impact_note: Some("会向 Exa 远程服务发送搜索请求。"),
            transport: MCPServerTransport::Http,
            command: None,
            args: None,
            url: Some("https://mcp.exa.ai/mcp?tools=web_search_exa"),
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "github",
            name: "GitHub MCP",
            description: "Manage GitHub repositories, issues, pull requests, and code reviews",
            category: "integration",
            capability_summary: "读取并操作 GitHub 仓库、Issue、PR 与代码审查上下文。",
            capability_tags: vec!["repository", "issues", "pull_requests"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("通常需要 GitHub Token。"),
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
            category: "workspace",
            capability_summary: "读取和写入指定目录下的文件与目录结构。",
            capability_tags: vec!["files", "workspace", "read_write"],
            impact_tags: vec!["workspace_read", "workspace_write"],
            impact_note: Some("可直接访问本地文件系统。"),
            transport: MCPServerTransport::Stdio,
            command: Some("npx"),
            args: Some(vec!["-y", "@modelcontextprotocol/server-filesystem"]),
            url: None,
            env_placeholders: vec![],
            headers_placeholders: vec![],
            default_timeout: Some(60000),
        },
        MCPTemplate {
            id: "postgres",
            name: "PostgreSQL MCP",
            description: "Query and manage PostgreSQL databases with SQL execution capabilities",
            category: "workspace",
            capability_summary: "对 PostgreSQL 数据库执行查询和管理操作。",
            capability_tags: vec!["database", "sql", "query"],
            impact_tags: vec!["network", "credentials", "data_access"],
            impact_note: Some("会访问数据库并读取或修改数据。"),
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
            category: "docs",
            capability_summary: "检索官方文档与代码示例，适合框架/API 查阅。",
            capability_tags: vec!["docs", "code_examples", "reference"],
            impact_tags: vec!["network", "remote_api"],
            impact_note: Some("会请求远程文档检索服务。"),
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
            category: "integration",
            capability_summary: "连接远程 MCP 网关，聚合工作流与集成能力。",
            capability_tags: vec!["remote_tools", "workflow", "integration"],
            impact_tags: vec!["network", "remote_api", "credentials"],
            impact_note: Some("通过远程 HTTP MCP 服务执行能力。"),
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
            category: "automation",
            capability_summary: "为助手保存和检索长期记忆。",
            capability_tags: vec!["memory", "storage", "retrieval"],
            impact_tags: vec!["local_state"],
            impact_note: Some("会保存额外记忆上下文。"),
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
            category: "automation",
            capability_summary: "提供分步推理工具，适合复杂问题拆解。",
            capability_tags: vec!["reasoning", "planning"],
            impact_tags: vec!["local_state"],
            impact_note: None,
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
            category: "automation",
            capability_summary: "提供示例工具、资源和提示，用于协议调试与验证。",
            capability_tags: vec!["sample", "debug", "reference"],
            impact_tags: vec!["local_state"],
            impact_note: None,
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
            category: "automation",
            capability_summary: "调试 MCP 交互与协议细节。",
            capability_tags: vec!["debug", "inspection"],
            impact_tags: vec!["local_state"],
            impact_note: None,
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
            category: "workspace",
            capability_summary: "读取 PDF 内容并提取结构化文本。",
            capability_tags: vec!["pdf", "document", "extract"],
            impact_tags: vec!["workspace_read"],
            impact_note: Some("会读取本地 PDF 文件。"),
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
            category: "workspace",
            capability_summary: "处理转录文本、字幕和对话内容。",
            capability_tags: vec!["transcript", "analysis"],
            impact_tags: vec!["workspace_read"],
            impact_note: Some("会读取本地文本资源。"),
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
            category: "docs",
            capability_summary: "检索 wiki 风格的知识库内容。",
            capability_tags: vec!["wiki", "knowledge", "reference"],
            impact_tags: vec!["network"],
            impact_note: None,
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
            category: "workspace",
            capability_summary: "读取本机运行状态、性能指标与诊断信息。",
            capability_tags: vec!["system", "metrics", "diagnostics"],
            impact_tags: vec!["local_state", "workspace_read"],
            impact_note: Some("会读取本机运行时信息。"),
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
            category: "search",
            capability_summary: "通过 Brave Search 检索网页与新闻结果。",
            capability_tags: vec!["web_search", "current_info"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("会使用 Brave Search API。"),
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
            category: "integration",
            capability_summary: "读取与操作 Slack 消息、频道和团队上下文。",
            capability_tags: vec!["chat", "team", "messages"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("会访问 Slack 工作区内容。"),
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
            category: "integration",
            capability_summary: "读取并操作 GitLab 项目、Issue 与 MR。",
            capability_tags: vec!["repository", "issues", "merge_requests"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("通常需要 GitLab Token。"),
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
            category: "integration",
            capability_summary: "调用地图、地理编码和路线规划能力。",
            capability_tags: vec!["maps", "places", "routing"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("会调用 Google Maps API。"),
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
            category: "workspace",
            capability_summary: "查询和操作 Redis 数据。",
            capability_tags: vec!["database", "cache", "query"],
            impact_tags: vec!["network", "credentials", "data_access"],
            impact_note: Some("会访问 Redis 实例。"),
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
            category: "docs",
            capability_summary: "从 AWS 知识库中检索 RAG 文档内容。",
            capability_tags: vec!["docs", "rag", "knowledge_base"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("会访问 AWS 远程知识库。"),
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
            category: "integration",
            capability_summary: "读取 Google Drive 文件与文档元数据。",
            capability_tags: vec!["documents", "storage", "drive"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("会访问 Google Drive 内容。"),
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
            category: "integration",
            capability_summary: "调用远程图像生成与编辑能力。",
            capability_tags: vec!["images", "generation"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("会请求远程生成式图像服务。"),
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
            category: "automation",
            capability_summary: "驱动浏览器自动化并提取页面内容。",
            capability_tags: vec!["browser", "automation", "scraping"],
            impact_tags: vec!["browser_automation", "network"],
            impact_note: Some("会启动本地浏览器自动化。"),
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
            category: "automation",
            capability_summary: "驱动 Playwright 浏览器自动化流程。",
            capability_tags: vec!["browser", "automation", "testing"],
            impact_tags: vec!["browser_automation", "network"],
            impact_note: Some("会启动本地浏览器自动化。"),
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
            category: "integration",
            capability_summary: "读取 Figma 设计文件与节点信息。",
            capability_tags: vec!["design", "figma", "documents"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("会访问 Figma 设计上下文。"),
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
            category: "integration",
            capability_summary: "读取和管理 Linear 项目与任务。",
            capability_tags: vec!["issues", "planning", "projects"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("会访问 Linear 工作区数据。"),
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
            category: "docs",
            capability_summary: "检索代码仓库上下文和代码情报信息。",
            capability_tags: vec!["code_search", "repository", "reference"],
            impact_tags: vec!["network", "credentials"],
            impact_note: Some("会访问远程代码仓库上下文。"),
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
            category: "search",
            capability_summary: "检索天气与预报信息。",
            capability_tags: vec!["weather", "forecast", "current_info"],
            impact_tags: vec!["network"],
            impact_note: Some("会访问远程天气数据源。"),
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

pub fn find_mcp_template_for_server(server: &MCPServer) -> Option<MCPTemplate> {
    get_mcp_templates()
        .into_iter()
        .find(|template| server_matches_template(server, template))
}

fn server_matches_template(server: &MCPServer, template: &MCPTemplate) -> bool {
    if server.id == format!("mcp-{}", template.id) {
        return true;
    }
    if server.config_key.as_deref() == Some(template.id) {
        return true;
    }

    let command = server
        .command
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let args_joined = server
        .args
        .as_ref()
        .map(|args| args.join(" ").to_lowercase())
        .unwrap_or_default();
    let url = server
        .http_url
        .as_ref()
        .or(server.url.as_ref())
        .map(|value| value.to_lowercase())
        .unwrap_or_default();

    if let Some(template_url) = template.url {
        let template_url = template_url.to_lowercase();
        if !template_url.is_empty()
            && url.contains(template_url.split('?').next().unwrap_or_default())
        {
            return true;
        }
    }

    if let Some(template_command) = template.command {
        if command == template_command.to_lowercase() {
            if let Some(args) = &template.args {
                if args
                    .iter()
                    .all(|arg| args_joined.contains(&arg.to_lowercase()))
                {
                    return true;
                }
            }
        }
    }

    match template.id {
        "exa" => {
            url.contains("mcp.exa.ai/mcp")
                || args_joined.contains("exa-mcp-server")
                || (args_joined.contains("mcp-remote") && args_joined.contains("mcp.exa.ai/mcp"))
        }
        "context7" => args_joined.contains("@upstash/context7-mcp"),
        _ => false,
    }
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

    let env = if template.env_placeholders.is_empty() {
        None
    } else {
        let mut env_map = HashMap::new();
        for placeholder in &template.env_placeholders {
            env_map.insert(placeholder.to_string(), format!("${}", placeholder));
        }
        Some(env_map)
    };

    let headers = if template.headers_placeholders.is_empty() {
        None
    } else {
        let mut headers_map = HashMap::new();
        for placeholder in &template.headers_placeholders {
            headers_map.insert(placeholder.to_string(), "${}".to_string());
        }
        Some(headers_map)
    };

    Ok(MCPServer {
        id: format!("mcp-{}", template.id),
        name: template.name.to_string(),
        config_key: Some(template.id.to_string()),
        description: Some(template.description.to_string()),
        transport: template.transport.clone(),
        command: template.command.map(String::from),
        args: template
            .args
            .as_ref()
            .map(|args| args.iter().map(|s| s.to_string()).collect()),
        cwd: None,
        url: template.url.map(String::from),
        http_url: template
            .url
            .filter(|_| {
                matches!(
                    template.transport,
                    MCPServerTransport::Http | MCPServerTransport::Sse
                )
            })
            .map(String::from),
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
                "category": t.category,
                "capability_summary": t.capability_summary,
                "capability_tags": t.capability_tags,
                "impact_tags": t.impact_tags,
                "impact_note": t.impact_note,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exa_template_exposes_search_metadata() {
        let exa = get_mcp_templates()
            .into_iter()
            .find(|template| template.id == "exa")
            .expect("exa template");

        assert_eq!(exa.category, "search");
        assert_eq!(exa.url, Some("https://mcp.exa.ai/mcp?tools=web_search_exa"));
        assert!(exa.capability_tags.contains(&"web_search"));
        assert!(exa.impact_tags.contains(&"remote_api"));
    }

    #[test]
    fn find_template_matches_context7_stdio_server() {
        let server = MCPServer {
            id: "mcp-context7".to_string(),
            name: "Context7".to_string(),
            config_key: Some("context7".to_string()),
            description: None,
            transport: MCPServerTransport::Stdio,
            command: Some("npx".to_string()),
            args: Some(vec!["-y".to_string(), "@upstash/context7-mcp".to_string()]),
            cwd: None,
            url: None,
            http_url: None,
            env: None,
            headers: None,
            timeout: Some(60_000),
            trust: Some(false),
            linked_provider_ids: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let matched = find_mcp_template_for_server(&server).expect("context7 template");
        assert_eq!(matched.id, "context7");
        assert_eq!(matched.category, "docs");
    }
}
