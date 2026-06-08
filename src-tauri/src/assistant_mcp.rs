use crate::mcp_runtime::McpClient;
use crate::mcp_servers::{self, MCPServer, MCPServerTransport};
use crate::mcp_templates::{find_mcp_template_for_server, get_mcp_template};
use crate::{atomic_write_string, get_data_dir};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const PREVIEW_CACHE_FILE: &str = "assistant_mcp_tool_previews.json";
const DEFAULT_ASSISTANT_TEMPLATE_IDS: [&str; 2] = ["exa", "context7"];

static PREVIEW_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn preview_lock() -> &'static Mutex<()> {
    PREVIEW_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpCategory {
    Search,
    Docs,
    Workspace,
    Integration,
    Automation,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum McpImpactTag {
    Network,
    RemoteApi,
    Credentials,
    WorkspaceRead,
    WorkspaceWrite,
    DataAccess,
    LocalState,
    BrowserAutomation,
    Trusted,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct McpToolPreviewItem {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct McpToolPreview {
    pub status: String,
    #[serde(default)]
    pub checked_at: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub tool_count: usize,
    #[serde(default)]
    pub tools: Vec<McpToolPreviewItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ManagedMcpServerCatalogItem {
    pub server_id: String,
    #[serde(default)]
    pub config_key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub transport: String,
    pub category: McpCategory,
    #[serde(default)]
    pub capability_summary: String,
    #[serde(default)]
    pub capability_tags: Vec<String>,
    #[serde(default)]
    pub impact_tags: Vec<McpImpactTag>,
    #[serde(default)]
    pub impact_note: Option<String>,
    pub tool_preview: McpToolPreview,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ManagedMcpCatalogResponse {
    #[serde(default)]
    pub default_server_ids: Vec<String>,
    #[serde(default)]
    pub items: Vec<ManagedMcpServerCatalogItem>,
}

fn preview_cache_path() -> Result<PathBuf, String> {
    let dir = get_data_dir()?.join("data").join("mcp");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir.join(PREVIEW_CACHE_FILE))
}

fn load_preview_cache() -> Result<HashMap<String, McpToolPreview>, String> {
    let _guard = preview_lock()
        .lock()
        .map_err(|_| "assistant MCP preview cache lock poisoned".to_string())?;
    let path = preview_cache_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    if raw.trim().is_empty() {
        return Ok(HashMap::new());
    }
    serde_json::from_str::<HashMap<String, McpToolPreview>>(&raw).map_err(|error| error.to_string())
}

fn save_preview_cache(cache: &HashMap<String, McpToolPreview>) -> Result<(), String> {
    let _guard = preview_lock()
        .lock()
        .map_err(|_| "assistant MCP preview cache lock poisoned".to_string())?;
    let path = preview_cache_path()?;
    let content = serde_json::to_string_pretty(cache).map_err(|error| error.to_string())?;
    atomic_write_string(&path, &content)
}

pub fn ensure_default_assistant_mcp_server_ids() -> Result<Vec<String>, String> {
    let mut resolved_ids = Vec::new();
    for template_id in DEFAULT_ASSISTANT_TEMPLATE_IDS {
        resolved_ids.push(ensure_template_server(template_id)?);
    }
    Ok(resolved_ids)
}

fn ensure_template_server(template_id: &str) -> Result<String, String> {
    let state = mcp_servers::get_mcp_servers()?;
    if let Some(existing) = state.servers.iter().find(|server| {
        find_mcp_template_for_server(server)
            .as_ref()
            .map(|template| template.id)
            == Some(template_id)
    }) {
        return Ok(existing.id.clone());
    }

    let mut server = get_mcp_template(template_id.to_string())?;
    server.id = format!("mcp-{}", template_id);
    server.config_key = Some(template_id.to_string());
    mcp_servers::save_mcp_server_internal(server.clone())?;
    Ok(server.id)
}

pub fn category_for_server(server: &MCPServer) -> McpCategory {
    if let Some(template) = find_mcp_template_for_server(server) {
        return category_from_template(template.category);
    }

    let signature = server_signature(server);
    if signature.contains("exa")
        || signature.contains("brave-search")
        || signature.contains("weather")
        || signature.contains("search")
    {
        return McpCategory::Search;
    }
    if signature.contains("context7")
        || signature.contains("docs")
        || signature.contains("wiki")
        || signature.contains("kb")
        || signature.contains("octocode")
    {
        return McpCategory::Docs;
    }
    if signature.contains("filesystem")
        || signature.contains("postgres")
        || signature.contains("redis")
        || signature.contains("pdf")
        || signature.contains("transcript")
        || signature.contains("system-monitor")
    {
        return McpCategory::Workspace;
    }
    if signature.contains("memory")
        || signature.contains("sequential")
        || signature.contains("playwright")
        || signature.contains("puppeteer")
        || signature.contains("debug")
        || signature.contains("everything")
    {
        return McpCategory::Automation;
    }
    McpCategory::Integration
}

pub fn build_catalog_item(
    server: &MCPServer,
    preview_cache: &HashMap<String, McpToolPreview>,
) -> ManagedMcpServerCatalogItem {
    let template = find_mcp_template_for_server(server);
    let category = template
        .as_ref()
        .map(|item| category_from_template(item.category))
        .unwrap_or_else(|| category_for_server(server));
    let impact_tags = template
        .as_ref()
        .map(|item| {
            item.impact_tags
                .iter()
                .filter_map(|tag| impact_tag_from_template(tag))
                .collect::<Vec<_>>()
        })
        .filter(|tags| !tags.is_empty())
        .unwrap_or_else(|| infer_impact_tags(server));

    let description = server
        .description
        .clone()
        .or_else(|| template.as_ref().map(|item| item.description.to_string()))
        .unwrap_or_default();

    let capability_summary = template
        .as_ref()
        .map(|item| item.capability_summary.to_string())
        .unwrap_or_else(|| {
            if !description.trim().is_empty() {
                description.clone()
            } else {
                format!(
                    "{} tools and integrations bound to this assistant.",
                    server.name
                )
            }
        });

    let capability_tags = template
        .as_ref()
        .map(|item| {
            item.capability_tags
                .iter()
                .map(|tag| tag.to_string())
                .collect()
        })
        .unwrap_or_else(|| infer_capability_tags(server, category));

    ManagedMcpServerCatalogItem {
        server_id: server.id.clone(),
        config_key: server.config_key.clone().unwrap_or_default(),
        name: server.name.clone(),
        description,
        transport: transport_label(server.transport.clone()),
        category,
        capability_summary,
        capability_tags,
        impact_tags,
        impact_note: template.and_then(|item| item.impact_note.map(|note| note.to_string())),
        tool_preview: preview_cache
            .get(&server.id)
            .cloned()
            .unwrap_or_else(|| unchecked_preview()),
    }
}

fn transport_label(transport: MCPServerTransport) -> String {
    match transport {
        MCPServerTransport::Stdio => "stdio",
        MCPServerTransport::Http => "http",
        MCPServerTransport::Sse => "sse",
    }
    .to_string()
}

fn unchecked_preview() -> McpToolPreview {
    McpToolPreview {
        status: "unchecked".to_string(),
        checked_at: None,
        error: None,
        tool_count: 0,
        tools: Vec::new(),
    }
}

fn failed_preview(error: String) -> McpToolPreview {
    McpToolPreview {
        status: "failed".to_string(),
        checked_at: Some(now_ts()),
        error: Some(compact_error(&error)),
        tool_count: 0,
        tools: Vec::new(),
    }
}

fn ready_preview(tools: Vec<crate::mcp_runtime::McpRuntimeTool>) -> McpToolPreview {
    McpToolPreview {
        status: "ready".to_string(),
        checked_at: Some(now_ts()),
        error: None,
        tool_count: tools.len(),
        tools: tools
            .into_iter()
            .map(|tool| McpToolPreviewItem {
                name: tool.name,
                description: tool.description,
            })
            .collect(),
    }
}

fn compact_error(error: &str) -> String {
    error
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

fn category_from_template(value: &str) -> McpCategory {
    match value {
        "search" => McpCategory::Search,
        "docs" => McpCategory::Docs,
        "workspace" => McpCategory::Workspace,
        "automation" => McpCategory::Automation,
        _ => McpCategory::Integration,
    }
}

fn impact_tag_from_template(value: &str) -> Option<McpImpactTag> {
    match value {
        "network" => Some(McpImpactTag::Network),
        "remote_api" => Some(McpImpactTag::RemoteApi),
        "credentials" => Some(McpImpactTag::Credentials),
        "workspace_read" => Some(McpImpactTag::WorkspaceRead),
        "workspace_write" => Some(McpImpactTag::WorkspaceWrite),
        "data_access" => Some(McpImpactTag::DataAccess),
        "local_state" => Some(McpImpactTag::LocalState),
        "browser_automation" => Some(McpImpactTag::BrowserAutomation),
        "trusted" => Some(McpImpactTag::Trusted),
        _ => None,
    }
}

fn infer_capability_tags(server: &MCPServer, category: McpCategory) -> Vec<String> {
    let mut tags = Vec::new();
    match category {
        McpCategory::Search => tags.extend(["web_search", "current_info"]),
        McpCategory::Docs => tags.extend(["docs", "reference"]),
        McpCategory::Workspace => tags.extend(["workspace", "data"]),
        McpCategory::Integration => tags.extend(["integration"]),
        McpCategory::Automation => tags.extend(["automation"]),
    }

    let signature = server_signature(server);
    if signature.contains("browser")
        || signature.contains("playwright")
        || signature.contains("puppeteer")
    {
        tags.push("browser");
    }
    if signature.contains("code") || signature.contains("repo") || signature.contains("github") {
        tags.push("code");
    }
    unique_strings(tags.into_iter().map(str::to_string).collect())
}

fn infer_impact_tags(server: &MCPServer) -> Vec<McpImpactTag> {
    let mut tags = Vec::new();
    match server.transport {
        MCPServerTransport::Http | MCPServerTransport::Sse => {
            tags.push(McpImpactTag::Network);
            tags.push(McpImpactTag::RemoteApi);
        }
        MCPServerTransport::Stdio => {
            tags.push(McpImpactTag::LocalState);
        }
    }

    if server
        .env
        .as_ref()
        .map(|env| !env.is_empty())
        .unwrap_or(false)
        || server
            .headers
            .as_ref()
            .map(|headers| !headers.is_empty())
            .unwrap_or(false)
    {
        tags.push(McpImpactTag::Credentials);
    }

    if server.trust.unwrap_or(false) {
        tags.push(McpImpactTag::Trusted);
    }

    let signature = server_signature(server);
    if signature.contains("filesystem")
        || signature.contains("pdf")
        || signature.contains("transcript")
        || signature.contains("system-monitor")
    {
        tags.push(McpImpactTag::WorkspaceRead);
    }
    if signature.contains("filesystem") {
        tags.push(McpImpactTag::WorkspaceWrite);
    }
    if signature.contains("postgres") || signature.contains("redis") {
        tags.push(McpImpactTag::DataAccess);
    }
    if signature.contains("playwright") || signature.contains("puppeteer") {
        tags.push(McpImpactTag::BrowserAutomation);
    }

    unique_impact_tags(tags)
}

fn unique_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn unique_impact_tags(values: Vec<McpImpactTag>) -> Vec<McpImpactTag> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(*value))
        .collect()
}

fn server_signature(server: &MCPServer) -> String {
    let command = server.command.clone().unwrap_or_default();
    let args = server.args.clone().unwrap_or_default().join(" ");
    let url = server
        .http_url
        .clone()
        .or_else(|| server.url.clone())
        .unwrap_or_default();
    format!("{} {} {} {} {}", server.id, server.name, command, args, url).to_lowercase()
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

async fn preview_server(server: &MCPServer) -> McpToolPreview {
    match McpClient::connect(server).await {
        Ok(mut client) => {
            let result = client.list_tools().await;
            client.close().await;
            match result {
                Ok(tools) => ready_preview(tools),
                Err(error) => failed_preview(error),
            }
        }
        Err(error) => failed_preview(error),
    }
}

#[tauri::command]
pub fn workspace_assistant_mcp_catalog() -> Result<ManagedMcpCatalogResponse, String> {
    let default_server_ids = ensure_default_assistant_mcp_server_ids()?;
    let state = mcp_servers::get_mcp_servers()?;
    let preview_cache = load_preview_cache()?;
    let mut items = state
        .servers
        .iter()
        .map(|server| build_catalog_item(server, &preview_cache))
        .collect::<Vec<_>>();
    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(ManagedMcpCatalogResponse {
        default_server_ids,
        items,
    })
}

#[tauri::command]
pub async fn mcp_tool_preview_refresh(
    server_ids: Option<Vec<String>>,
) -> Result<Vec<ManagedMcpServerCatalogItem>, String> {
    let _ = ensure_default_assistant_mcp_server_ids()?;
    let state = mcp_servers::get_mcp_servers()?;
    let target_ids = server_ids.unwrap_or_else(|| {
        state
            .servers
            .iter()
            .map(|server| server.id.clone())
            .collect()
    });
    let target_set = target_ids.into_iter().collect::<HashSet<_>>();

    let mut preview_cache = load_preview_cache()?;
    let mut refreshed = Vec::new();
    for server in state
        .servers
        .iter()
        .filter(|server| target_set.contains(&server.id))
    {
        let preview = preview_server(server).await;
        preview_cache.insert(server.id.clone(), preview.clone());
        refreshed.push(build_catalog_item(server, &preview_cache));
    }

    save_preview_cache(&preview_cache)?;
    refreshed.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(refreshed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_with_args(id: &str, name: &str, args: &[&str]) -> MCPServer {
        MCPServer {
            id: id.to_string(),
            name: name.to_string(),
            config_key: Some(name.to_lowercase().replace(' ', "-")),
            description: None,
            transport: MCPServerTransport::Stdio,
            command: Some("npx".to_string()),
            args: Some(args.iter().map(|value| value.to_string()).collect()),
            cwd: None,
            url: None,
            http_url: None,
            env: None,
            headers: None,
            timeout: Some(60_000),
            trust: Some(false),
            linked_provider_ids: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn category_detection_prefers_known_template_matches() {
        let exa = MCPServer {
            id: "custom".to_string(),
            name: "Exa Search".to_string(),
            config_key: Some("exa".to_string()),
            description: None,
            transport: MCPServerTransport::Http,
            command: None,
            args: None,
            cwd: None,
            url: Some("https://mcp.exa.ai/mcp?tools=web_search_exa".to_string()),
            http_url: Some("https://mcp.exa.ai/mcp?tools=web_search_exa".to_string()),
            env: None,
            headers: None,
            timeout: Some(60_000),
            trust: Some(false),
            linked_provider_ids: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        assert_eq!(category_for_server(&exa), McpCategory::Search);
    }

    #[test]
    fn non_template_servers_infer_impact_tags() {
        let filesystem = server_with_args(
            "mcp-files",
            "Files",
            &["-y", "@modelcontextprotocol/server-filesystem"],
        );
        let impacts = infer_impact_tags(&filesystem);
        assert!(impacts.contains(&McpImpactTag::WorkspaceRead));
        assert!(impacts.contains(&McpImpactTag::WorkspaceWrite));
    }

    #[test]
    fn build_catalog_item_uses_cached_preview_and_template_metadata() {
        let server = MCPServer {
            id: "mcp-exa".to_string(),
            name: "Exa MCP".to_string(),
            config_key: Some("exa".to_string()),
            description: None,
            transport: MCPServerTransport::Http,
            command: None,
            args: None,
            cwd: None,
            url: Some("https://mcp.exa.ai/mcp?tools=web_search_exa".to_string()),
            http_url: Some("https://mcp.exa.ai/mcp?tools=web_search_exa".to_string()),
            env: None,
            headers: None,
            timeout: Some(60_000),
            trust: Some(false),
            linked_provider_ids: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let mut cache = HashMap::new();
        cache.insert(
            server.id.clone(),
            McpToolPreview {
                status: "ready".to_string(),
                checked_at: Some(123),
                error: None,
                tool_count: 1,
                tools: vec![McpToolPreviewItem {
                    name: "web_search_exa".to_string(),
                    description: "Search the web".to_string(),
                }],
            },
        );

        let item = build_catalog_item(&server, &cache);
        assert_eq!(item.category, McpCategory::Search);
        assert_eq!(item.tool_preview.status, "ready");
        assert_eq!(item.tool_preview.tool_count, 1);
        assert!(item.capability_tags.iter().any(|tag| tag == "web_search"));
    }

    #[test]
    fn non_template_http_servers_infer_network_and_credentials_tags() {
        let server = MCPServer {
            id: "remote-docs".to_string(),
            name: "Remote Docs".to_string(),
            config_key: Some("remote-docs".to_string()),
            description: Some("Custom docs bridge".to_string()),
            transport: MCPServerTransport::Http,
            command: None,
            args: None,
            cwd: None,
            url: Some("https://example.com/mcp".to_string()),
            http_url: Some("https://example.com/mcp".to_string()),
            env: None,
            headers: Some(HashMap::from([(
                "Authorization".to_string(),
                "Bearer test".to_string(),
            )])),
            timeout: Some(60_000),
            trust: Some(true),
            linked_provider_ids: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let impacts = infer_impact_tags(&server);
        assert!(impacts.contains(&McpImpactTag::Network));
        assert!(impacts.contains(&McpImpactTag::RemoteApi));
        assert!(impacts.contains(&McpImpactTag::Credentials));
        assert!(impacts.contains(&McpImpactTag::Trusted));
    }
}
