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
