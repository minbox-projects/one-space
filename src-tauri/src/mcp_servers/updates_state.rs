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

fn load_updates_state() -> Result<MCPUpdatesState, String> {
    let path = get_updates_state_path()?;
    if !path.exists() {
        return Ok(MCPUpdatesState {
            status: "idle".to_string(),
            ..MCPUpdatesState::default()
        });
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Ok(MCPUpdatesState {
            status: "idle".to_string(),
            ..MCPUpdatesState::default()
        });
    }
    let mut state = serde_json::from_str::<MCPUpdatesState>(&raw).map_err(|e| e.to_string())?;
    if state.status.trim().is_empty() {
        state.status = "idle".to_string();
    }
    Ok(state)
}

fn save_updates_state(state: &MCPUpdatesState) -> Result<(), String> {
    let path = get_updates_state_path()?;
    let content = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    atomic_write(&path, &content)
}

#[derive(Debug, Clone)]
struct ParsedNpmSpec {
    package_name: String,
    version: Option<String>,
    token_index: usize,
}

fn parse_npm_package_spec(spec: &str) -> Option<(String, Option<String>)> {
    let trimmed = spec.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return None;
    }

    if trimmed.starts_with('@') {
        let slash_idx = trimmed.find('/')?;
        let suffix = &trimmed[(slash_idx + 1)..];
        if suffix.is_empty() {
            return None;
        }
        if let Some(version_idx) = trimmed.rfind('@') {
            if version_idx > slash_idx + 1 {
                let pkg = trimmed[..version_idx].to_string();
                let version = trimmed[(version_idx + 1)..].trim().to_string();
                if version.is_empty() {
                    return None;
                }
                return Some((pkg, Some(version)));
            }
        }
        return Some((trimmed.to_string(), None));
    }

    if let Some(version_idx) = trimmed.rfind('@') {
        if version_idx > 0 {
            let pkg = trimmed[..version_idx].trim().to_string();
            let version = trimmed[(version_idx + 1)..].trim().to_string();
            if pkg.is_empty() || version.is_empty() {
                return None;
            }
            return Some((pkg, Some(version)));
        }
    }

    Some((trimmed.to_string(), None))
}

fn first_npx_package_token(args: &[String]) -> Option<usize> {
    for (idx, arg) in args.iter().enumerate() {
        if arg == "--" {
            let next = idx + 1;
            if next < args.len() {
                return Some(next);
            }
            return None;
        }
        if arg.starts_with('-') {
            continue;
        }
        return Some(idx);
    }
    None
}

fn parse_server_npm_spec(server: &MCPServer) -> Option<ParsedNpmSpec> {
    if server.transport != MCPServerTransport::Stdio {
        return None;
    }
    let command = server.command.as_ref()?.trim().to_lowercase();
    if command != "npx" {
        return None;
    }
    let args = server.args.as_ref()?;
    let token_index = first_npx_package_token(args)?;
    let token = args.get(token_index)?;
    let (package_name, version) = parse_npm_package_spec(token)?;
    Some(ParsedNpmSpec {
        package_name,
        version,
        token_index,
    })
}

fn parse_semver_parts(input: &str) -> Option<Vec<u64>> {
    let normalized = input.trim().trim_start_matches('v');
    if normalized.is_empty() {
        return None;
    }
    let main = normalized
        .split(['-', '+'])
        .next()
        .map(str::trim)
        .unwrap_or("");
    if main.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for seg in main.split('.') {
        if seg.is_empty() {
            return None;
        }
        let digits = seg
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() {
            return None;
        }
        let num = digits.parse::<u64>().ok()?;
        out.push(num);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn compare_semver_like(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let pa = parse_semver_parts(a)?;
    let pb = parse_semver_parts(b)?;
    let max_len = pa.len().max(pb.len());
    for idx in 0..max_len {
        let va = *pa.get(idx).unwrap_or(&0);
        let vb = *pb.get(idx).unwrap_or(&0);
        if va < vb {
            return Some(std::cmp::Ordering::Less);
        }
        if va > vb {
            return Some(std::cmp::Ordering::Greater);
        }
    }
    Some(std::cmp::Ordering::Equal)
}

fn scoped_package_url_name(package_name: &str) -> String {
    if package_name.starts_with('@') {
        package_name.replace('/', "%2f")
    } else {
        package_name.to_string()
    }
}

async fn fetch_npm_latest_version(
    client: &reqwest::Client,
    package_name: &str,
) -> Result<String, String> {
    let url = format!(
        "https://registry.npmjs.org/{}",
        scoped_package_url_name(package_name)
    );
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;
    if !res.status().is_success() {
        return Err(format!("registry status: {}", res.status()));
    }
    let data = res
        .json::<Value>()
        .await
        .map_err(|e| format!("invalid response: {}", e))?;
    data.get("dist-tags")
        .and_then(|v| v.get("latest"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or("missing dist-tags.latest".to_string())
}
