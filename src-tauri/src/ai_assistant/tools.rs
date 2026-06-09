fn parse_tool_call_arguments(raw_arguments: Option<&str>) -> Value {
    let Some(raw) = raw_arguments
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Value::Null;
    };

    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

fn schema_expects_object(schema: Option<&Value>) -> bool {
    schema
        .and_then(|value| value.get("type"))
        .and_then(|value| value.as_str())
        .map(|value| value == "object")
        .unwrap_or_else(|| {
            schema
                .map(|value| value.get("properties").is_some() || value.get("required").is_some())
                .unwrap_or(false)
        })
}

fn schema_property<'a>(schema: Option<&'a Value>, field: &str) -> Option<&'a Value> {
    schema?.get("properties")?.get(field)
}

fn schema_property_allows_string(property: &Value) -> bool {
    property
        .get("type")
        .map(|value| match value {
            Value::String(kind) => kind == "string",
            Value::Array(items) => items.iter().any(|item| item.as_str() == Some("string")),
            _ => false,
        })
        .unwrap_or(false)
}

fn required_fields(schema: Option<&Value>) -> Vec<String> {
    schema
        .and_then(|value| value.get("required"))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn required_string_fields(schema: Option<&Value>) -> Vec<String> {
    required_fields(schema)
        .into_iter()
        .filter(|field| {
            schema_property(schema, field)
                .map(schema_property_allows_string)
                .unwrap_or(false)
        })
        .collect()
}

fn string_argument_value(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn is_missing_required_field(
    schema: Option<&Value>,
    field: &str,
    arguments: &Map<String, Value>,
) -> bool {
    match arguments.get(field) {
        None => true,
        Some(Value::Null) => true,
        Some(value) => {
            schema_property(schema, field)
                .map(schema_property_allows_string)
                .unwrap_or(false)
                && string_argument_value(value).is_none()
        }
    }
}

fn find_missing_required_fields(schema: Option<&Value>, arguments: &Value) -> Vec<String> {
    let required = required_fields(schema);
    if required.is_empty() {
        return Vec::new();
    }

    let Some(object) = arguments.as_object() else {
        return required;
    };

    required
        .into_iter()
        .filter(|field| is_missing_required_field(schema, field, object))
        .collect()
}

fn is_search_like_field_name(field_name: &str) -> bool {
    let normalized = field_name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect::<String>();
    matches!(
        normalized.as_str(),
        "q" | "query"
            | "search"
            | "searchquery"
            | "searchterm"
            | "keywords"
            | "keyword"
            | "topic"
            | "prompt"
    )
}

fn alias_string_candidate(arguments: &Map<String, Value>, field_name: &str) -> Option<String> {
    let alias_keys: &[&str] = if is_search_like_field_name(field_name) {
        &[
            "q",
            "query",
            "search",
            "search_query",
            "search_term",
            "input",
            "text",
            "prompt",
            "topic",
            "keywords",
        ]
    } else {
        &[]
    };

    alias_keys
        .iter()
        .find_map(|key| arguments.get(*key).and_then(string_argument_value))
}

fn single_string_candidate(arguments: &Map<String, Value>) -> Option<String> {
    let unique = arguments
        .values()
        .filter_map(string_argument_value)
        .collect::<std::collections::HashSet<_>>();
    if unique.len() == 1 {
        unique.into_iter().next()
    } else {
        None
    }
}

fn looks_like_structured_payload(raw: &str) -> bool {
    let trimmed = raw.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn normalize_tool_arguments(
    tool_name: &str,
    arguments: &Value,
    tool_definition: Option<&ToolDefinition>,
    fallback_user_text: Option<&str>,
) -> Result<Value, String> {
    let schema = tool_definition.and_then(|definition| definition.parameters.as_ref());
    let required_string_fields = required_string_fields(schema);
    let single_required_string_field = if required_string_fields.len() == 1 {
        required_string_fields.first().cloned()
    } else {
        None
    };

    let mut normalized = match arguments {
        Value::Object(map) => Value::Object(map.clone()),
        Value::String(raw) if schema_expects_object(schema) => {
            if let Some(field) = single_required_string_field.as_ref() {
                if !looks_like_structured_payload(raw) {
                    let mut map = Map::new();
                    if let Some(value) = string_argument_value(arguments) {
                        map.insert(field.clone(), Value::String(value));
                    }
                    Value::Object(map)
                } else {
                    Value::Object(Map::new())
                }
            } else {
                Value::Object(Map::new())
            }
        }
        Value::Null if schema_expects_object(schema) => Value::Object(Map::new()),
        value => value.clone(),
    };

    if let (Some(field), Some(object)) = (
        single_required_string_field.as_ref(),
        normalized.as_object_mut(),
    ) {
        if is_missing_required_field(schema, field, object) {
            if is_search_like_field_name(field) {
                if let Some(candidate) = alias_string_candidate(object, field) {
                    object.insert(field.clone(), Value::String(candidate));
                } else if let Some(fallback) = fallback_user_text
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    object.insert(field.clone(), Value::String(fallback.to_string()));
                }
            } else if let Some(candidate) = single_string_candidate(object) {
                object.insert(field.clone(), Value::String(candidate));
            }
        }
    }

    let missing = find_missing_required_fields(schema, &normalized);
    if !missing.is_empty() {
        let display_name = humanize_tool_name(tool_name);
        let display_name = if display_name.is_empty() {
            tool_name.to_string()
        } else {
            display_name
        };
        return Err(format!(
            "Tool '{}' is missing required arguments: {}.",
            display_name,
            missing.join(", ")
        ));
    }

    Ok(normalized)
}

fn build_reqwest_client(timeout_secs: Option<u64>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder();
    if let Some(timeout) = timeout_secs {
        builder = builder.timeout(std::time::Duration::from_secs(timeout));
    }
    builder.build().map_err(|e| e.to_string())
}

fn interval_minutes_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"每\s*(\d+)\s*(分钟|小时)").expect("valid interval regex"))
}

fn time_of_day_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?P<hour>\d{1,2})[:：](?P<minute>\d{2})").expect("valid time regex")
    })
}

fn quoted_name_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?:名为|叫做|叫|标题为|任务名(?:称)?(?:为)?)[\s:：]*["“]?([^"”\n]+)["”]?"#)
            .expect("valid quoted name regex")
    })
}

fn apply_provider_headers(
    request: reqwest::RequestBuilder,
    provider: &AiAssistantProvider,
) -> Result<reqwest::RequestBuilder, String> {
    let mut request = request;
    if !provider.api_key.trim().is_empty() {
        match provider.auth_scheme.as_str() {
            "x-api-key" => {
                request = request.header("x-api-key", provider.api_key.clone());
            }
            "x-goog-api-key" => {
                request = request.header("x-goog-api-key", provider.api_key.clone());
            }
            _ => {
                request = request.header(AUTHORIZATION, format!("Bearer {}", provider.api_key));
            }
        }
    }
    let mut header_map = HeaderMap::new();
    for header in &provider.extra_headers {
        if header.key.trim().is_empty() || header.value.trim().is_empty() {
            continue;
        }
        let key =
            HeaderName::from_bytes(header.key.trim().as_bytes()).map_err(|e| e.to_string())?;
        let value = HeaderValue::from_str(header.value.trim()).map_err(|e| e.to_string())?;
        header_map.insert(key, value);
    }
    Ok(request.headers(header_map))
}

fn resolve_endpoint(base_url: &str, suffix: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with(suffix) {
        trimmed.to_string()
    } else {
        format!("{trimmed}/{suffix}")
    }
}

fn normalize_openai_compatible_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let normalized = [
        "chat/completions",
        "responses",
        "completions",
        "embeddings",
        "audio/speech",
        "audio/transcriptions",
    ]
    .into_iter()
    .find_map(|suffix| trimmed.strip_suffix(suffix))
    .map(|prefix| prefix.trim_end_matches('/'))
    .unwrap_or(trimmed);

    normalized.to_string()
}

fn resolve_provider_endpoint(provider: &AiAssistantProvider, suffix: &str) -> String {
    match provider.protocol.as_str() {
        "openai-compatible" => {
            let normalized = normalize_openai_compatible_base_url(&provider.base_url);
            resolve_endpoint(&normalized, suffix)
        }
        _ => resolve_endpoint(&provider.base_url, suffix),
    }
}

fn build_builtin_tools(tool_policy: &AgentToolPolicy) -> Vec<ToolDefinition> {
    let mut tools = Vec::new();
    if tool_policy.workspace_read {
        tools.push(ToolDefinition {
            name: "workspace_read".to_string(),
            description: "Read a file from the workspace. Returns the file content.".to_string(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path to read (relative to workspace root or absolute)"
                    }
                },
                "required": ["path"]
            })),
        });
    }
    if tool_policy.notes_search {
        tools.push(ToolDefinition {
            name: "notes_search".to_string(),
            description: "Search through user's notes. Returns matching note fragments."
                .to_string(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to find in notes"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return",
                        "default": 5
                    }
                },
                "required": ["query"]
            })),
        });
    }
    tools
}

#[derive(Debug, Clone)]
struct BoundMcpTool {
    assistant_tool_name: String,
    server_id: String,
    server_name: String,
    config_key: String,
    original_tool_name: String,
    category: crate::assistant_mcp::McpCategory,
    definition: ToolDefinition,
}

fn humanize_tool_name(name: &str) -> String {
    let mut parts = Vec::new();
    for token in name
        .split(|ch: char| matches!(ch, '_' | '.' | '-' | '/' | ':' | ' '))
        .filter(|token| !token.is_empty())
    {
        let lower = token.to_ascii_lowercase();
        let word = match lower.as_str() {
            "mcp" => "MCP".to_string(),
            "api" => "API".to_string(),
            "url" => "URL".to_string(),
            "id" => "ID".to_string(),
            _ => {
                let mut chars = lower.chars();
                match chars.next() {
                    Some(first) => {
                        let mut word = String::new();
                        word.extend(first.to_uppercase());
                        word.push_str(chars.as_str());
                        word
                    }
                    None => String::new(),
                }
            }
        };
        if !word.is_empty() {
            parts.push(word);
        }
    }
    parts.join(" ")
}

fn build_tool_call_snapshot(
    id: String,
    name: String,
    arguments: Option<String>,
    status: impl Into<String>,
    summary: Option<String>,
    result: Option<String>,
    started_at: u64,
    finished_at: Option<u64>,
    binding: Option<&BoundMcpTool>,
) -> AssistantToolCall {
    let display_name = binding
        .map(|item| humanize_tool_name(&item.original_tool_name))
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let fallback = humanize_tool_name(&name);
            if fallback.is_empty() {
                None
            } else {
                Some(fallback)
            }
        });

    AssistantToolCall {
        id,
        name,
        display_name,
        arguments,
        server_id: binding.map(|item| item.server_id.clone()),
        server_name: binding.map(|item| item.server_name.clone()),
        original_tool_name: binding.map(|item| item.original_tool_name.clone()),
        status: status.into(),
        summary,
        result,
        started_at,
        finished_at,
    }
}

fn build_available_tools(
    tool_policy: &AgentToolPolicy,
    mcp_tools: &[BoundMcpTool],
) -> Vec<ToolDefinition> {
    let mut tools = build_builtin_tools(tool_policy);
    tools.extend(mcp_tools.iter().map(|item| item.definition.clone()));
    tools
}

async fn load_bound_mcp_tools(
    agent: Option<&AgentDefinition>,
    search_enabled: bool,
) -> Result<(HashMap<String, McpClient>, HashMap<String, BoundMcpTool>), String> {
    let Some(agent) = agent else {
        return Ok((HashMap::new(), HashMap::new()));
    };
    if agent.mcp_server_ids.is_empty() {
        return Ok((HashMap::new(), HashMap::new()));
    }

    let state = crate::mcp_servers::get_mcp_servers()?;
    let mut servers_by_id = HashMap::new();
    for server in state.servers {
        servers_by_id.insert(server.id.clone(), server);
    }

    let mut clients = HashMap::new();
    let mut tools = HashMap::new();
    let mut seen_servers = HashSet::new();

    for server_id in &agent.mcp_server_ids {
        if !seen_servers.insert(server_id.clone()) {
            continue;
        }
        let Some(server) = servers_by_id.get(server_id).cloned() else {
            continue;
        };
        let category = crate::assistant_mcp::category_for_server(&server);
        if matches!(category, crate::assistant_mcp::McpCategory::Search) && !search_enabled {
            continue;
        }

        let config_key = server
            .config_key
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| server.id.clone());

        let mut client = match McpClient::connect(&server).await {
            Ok(client) => client,
            Err(error) => {
                eprintln!(
                    "failed to initialize MCP server '{}': {}",
                    server.name, error
                );
                continue;
            }
        };

        let listed_tools = match client.list_tools().await {
            Ok(tools) => tools,
            Err(error) => {
                eprintln!(
                    "failed to list tools for MCP server '{}': {}",
                    server.name, error
                );
                client.close().await;
                continue;
            }
        };

        for tool in listed_tools {
            let assistant_tool_name = compose_mcp_tool_name(&config_key, &tool.name);
            tools.insert(
                assistant_tool_name.clone(),
                BoundMcpTool {
                    assistant_tool_name: assistant_tool_name.clone(),
                    server_id: server.id.clone(),
                    server_name: server.name.clone(),
                    config_key: config_key.clone(),
                    original_tool_name: tool.name.clone(),
                    category,
                    definition: ToolDefinition {
                        name: assistant_tool_name,
                        description: if tool.description.trim().is_empty() {
                            format!("MCP tool '{}' from {}", tool.name, server.name)
                        } else {
                            tool.description.clone()
                        },
                        parameters: Some(tool.input_schema.clone()),
                    },
                },
            );
        }

        clients.insert(server.id.clone(), client);
    }

    Ok((clients, tools))
}

async fn close_mcp_clients(clients: &mut HashMap<String, McpClient>) {
    let mut owned = clients
        .drain()
        .map(|(_, client)| client)
        .collect::<Vec<_>>();
    for client in &mut owned {
        client.close().await;
    }
}

fn is_exa_mcp_tool(binding: &BoundMcpTool) -> bool {
    binding.config_key == "exa"
        || binding.original_tool_name.contains("_exa")
        || (binding.server_name.to_lowercase().contains("exa")
            && matches!(binding.category, crate::assistant_mcp::McpCategory::Search))
}

fn extract_sources_from_mcp_output(
    binding: &BoundMcpTool,
    output: &McpToolCallOutput,
) -> Vec<AssistantMessageSource> {
    if !is_exa_mcp_tool(binding) {
        return Vec::new();
    }

    let from_value = output
        .structured_content
        .as_ref()
        .map(extract_sources_from_value)
        .filter(|items| !items.is_empty())
        .or_else(|| {
            let items = extract_sources_from_value(&output.raw_result);
            if items.is_empty() {
                None
            } else {
                Some(items)
            }
        });

    if let Some(items) = from_value {
        return items;
    }

    serde_json::from_str::<Value>(&output.text)
        .ok()
        .map(|value| extract_sources_from_value(&value))
        .unwrap_or_default()
}

fn extract_sources_from_value(value: &Value) -> Vec<AssistantMessageSource> {
    for pointer in [
        "/results",
        "/data/results",
        "/searchResults",
        "/data/searchResults",
        "/items",
        "/data/items",
    ] {
        if let Some(items) = value.pointer(pointer).and_then(|entry| entry.as_array()) {
            let collected = collect_sources_from_items(items);
            if !collected.is_empty() {
                return collected;
            }
        }
    }

    value
        .as_array()
        .map(|items| collect_sources_from_items(items))
        .unwrap_or_default()
}

fn collect_sources_from_items(items: &[Value]) -> Vec<AssistantMessageSource> {
    items
        .iter()
        .filter_map(|item| {
            let url = item
                .get("url")
                .or_else(|| item.get("id"))
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if url.is_empty() {
                return None;
            }

            let title = item
                .get("title")
                .or_else(|| item.get("name"))
                .and_then(|value| value.as_str())
                .unwrap_or_else(|| url.as_str())
                .trim()
                .to_string();

            let snippet = item
                .get("snippet")
                .or_else(|| item.get("text"))
                .or_else(|| item.get("summary"))
                .or_else(|| item.get("content"))
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    item.get("highlights")
                        .and_then(|value| value.as_array())
                        .map(|highlights| {
                            highlights
                                .iter()
                                .filter_map(|highlight| highlight.as_str())
                                .take(2)
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .filter(|value| !value.trim().is_empty())
                })
                .unwrap_or_default();

            Some(AssistantMessageSource {
                title,
                url,
                snippet,
            })
        })
        .take(6)
        .collect()
}

async fn execute_tool_call(
    app: &tauri::AppHandle,
    state: &AssistantState,
    tool_name: &str,
    arguments: &Value,
    conversation_id: &str,
    message_id: &str,
    tool_definitions: &HashMap<String, ToolDefinition>,
    mcp_tools: &HashMap<String, BoundMcpTool>,
    mcp_clients: &mut HashMap<String, McpClient>,
) -> Result<(String, Vec<AssistantMessageSource>), String> {
    let start = now_ts();
    let mcp_binding = mcp_tools.get(tool_name);
    let tool_definition = tool_definitions.get(tool_name);
    let effective_arguments = normalize_tool_arguments(
        mcp_binding
            .map(|binding| binding.original_tool_name.as_str())
            .unwrap_or(tool_name),
        arguments,
        tool_definition,
        latest_user_message_text(state, conversation_id).as_deref(),
    )?;
    let tool_id = uuid::Uuid::new_v4().to_string();
    let pending_tool = build_tool_call_snapshot(
        tool_id.clone(),
        tool_name.to_string(),
        Some(effective_arguments.to_string()),
        "running",
        None,
        None,
        start,
        None,
        mcp_binding,
    );

    emit_stream_event(
        app,
        AssistantStreamEvent {
            conversation_id: conversation_id.to_string(),
            message_id: message_id.to_string(),
            kind: "tool.started".to_string(),
            text: None,
            sources: None,
            tool: Some(pending_tool.clone()),
            error: None,
        },
    );

    let result = match tool_name {
        "workspace_read" => {
            let path = effective_arguments
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "workspace_read requires 'path' argument".to_string())?;

            let data_dir = crate::get_data_dir()?;
            let file_path = if path.starts_with('/') {
                path.to_string()
            } else if path.starts_with('~') {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                path.replacen('~', &home, 1)
            } else {
                data_dir.join(path).to_string_lossy().to_string()
            };

            fs::read_to_string(&file_path)
                .map(|content| (content, Vec::new()))
                .map_err(|e| format!("Failed to read file {}: {}", file_path, e))
        }
        "notes_search" => {
            let _query = effective_arguments
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "notes_search requires 'query' argument".to_string())?;
            let _limit = effective_arguments
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as usize;

            // TODO: Implement actual notes search after notes module integration
            Err("Notes search is not yet implemented. Please enable this feature in future updates.".to_string())
        }
        _ => {
            let binding = mcp_binding.ok_or_else(|| format!("Unknown tool: {}", tool_name))?;
            let client = mcp_clients
                .get_mut(&binding.server_id)
                .ok_or_else(|| format!("MCP server unavailable for tool '{}'", tool_name))?;
            client
                .call_tool(&binding.original_tool_name, effective_arguments.clone())
                .await
                .map(|output| {
                    let sources = extract_sources_from_mcp_output(binding, &output);
                    (output.text, sources)
                })
        }
    };

    match result {
        Ok((result_text, sources)) => {
            let done_tool = build_tool_call_snapshot(
                tool_id.clone(),
                tool_name.to_string(),
                Some(effective_arguments.to_string()),
                "success",
                Some("Tool executed successfully".to_string()),
                Some(result_text.clone()),
                start,
                Some(now_ts()),
                mcp_binding,
            );

            emit_stream_event(
                app,
                AssistantStreamEvent {
                    conversation_id: conversation_id.to_string(),
                    message_id: message_id.to_string(),
                    kind: "tool.finished".to_string(),
                    text: None,
                    sources: Some(sources.clone()),
                    tool: Some(done_tool.clone()),
                    error: None,
                },
            );

            Ok((result_text, sources))
        }
        Err(error) => {
            let failed_tool = build_tool_call_snapshot(
                tool_id.clone(),
                tool_name.to_string(),
                Some(effective_arguments.to_string()),
                "failed",
                Some(error.clone()),
                None,
                start,
                Some(now_ts()),
                mcp_binding,
            );

            emit_stream_event(
                app,
                AssistantStreamEvent {
                    conversation_id: conversation_id.to_string(),
                    message_id: message_id.to_string(),
                    kind: "tool.finished".to_string(),
                    text: None,
                    sources: None,
                    tool: Some(failed_tool.clone()),
                    error: Some(error.clone()),
                },
            );

            Err(error)
        }
    }
}
