use serde::{Deserialize, Serialize};
use std::time::Duration;

const VERSION_FETCH_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliUpdateInfo {
    pub tool: String,
    pub installed: bool,
    pub current_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_version_normalized: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<String>,
    pub latest_source: String,
    pub latest_url: String,
    pub update_available: bool,
    pub compare_status: String,
    pub update_command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliUpdateApplyResult {
    pub tool: String,
    pub success: bool,
    pub terminal_launched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[allow(dead_code)]
struct CliToolMetadata {
    tool: &'static str,
    cmd_name: &'static str,
    latest_source: &'static str,
    latest_url: &'static str,
    fallback_source: Option<&'static str>,
    fallback_url: Option<&'static str>,
    update_command: &'static str,
}

fn get_tool_metadata(tool: &str) -> Option<CliToolMetadata> {
    match tool {
        "claude" => Some(CliToolMetadata {
            tool: "claude",
            cmd_name: "claude",
            latest_source: "claude_release",
            latest_url: "https://downloads.claude.ai/claude-code-releases/latest",
            fallback_source: Some("npm_registry"),
            fallback_url: Some("https://registry.npmjs.org/@anthropic-ai%2Fclaude-code/latest"),
            update_command: "curl -fsSL https://claude.ai/install.sh | bash",
        }),
        "codex" => Some(CliToolMetadata {
            tool: "codex",
            cmd_name: "codex",
            latest_source: "npm_registry",
            latest_url: "https://registry.npmjs.org/@openai%2Fcodex/latest",
            fallback_source: None,
            fallback_url: None,
            update_command: "codex update",
        }),
        "antigravity" => Some(CliToolMetadata {
            tool: "antigravity",
            cmd_name: "agy",
            latest_source: "official_installer",
            latest_url: "https://antigravity.google/cli/install.sh",
            fallback_source: None,
            fallback_url: None,
            update_command: "curl -fsSL https://antigravity.google/cli/install.sh | bash",
        }),
        "opencode" => Some(CliToolMetadata {
            tool: "opencode",
            cmd_name: "opencode",
            latest_source: "github_release",
            latest_url: "https://api.github.com/repos/anomalyco/opencode/releases/latest",
            fallback_source: Some("npm_registry"),
            fallback_url: Some("https://registry.npmjs.org/opencode-ai/latest"),
            update_command: "curl -fsSL https://opencode.ai/install | bash",
        }),
        _ => None,
    }
}

fn validate_tool(tool: &str) -> Result<(), String> {
    match tool {
        "claude" | "codex" | "antigravity" | "opencode" => Ok(()),
        _ => Err(format!("Unknown tool: {}", tool)),
    }
}

fn get_update_command(tool: &str) -> Result<&'static str, String> {
    validate_tool(tool)?;
    Ok(get_tool_metadata(tool).map(|m| m.update_command).unwrap())
}

async fn fetch_npm_version(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .timeout(VERSION_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    parse_npm_latest_version(&json)
}

fn build_client() -> Result<reqwest::Client, String> {
    let proxy_mgr = crate::proxy::PROXY_MANAGER
        .get()
        .ok_or("Proxy manager not initialized")?;
    proxy_mgr
        .apply_builder(reqwest::Client::builder())?
        .build()
        .map_err(|e| e.to_string())
}

async fn fetch_primary_version(meta: &CliToolMetadata) -> Result<String, String> {
    let client = build_client()?;
    match meta.latest_source {
        "claude_release" => fetch_claude_release_version(&client, meta.latest_url).await,
        "npm_registry" => fetch_npm_version(&client, meta.latest_url).await,
        "github_release" => fetch_github_version(&client, meta.latest_url).await,
        "official_installer" => {
            Err("Antigravity is updated through its official installer".to_string())
        }
        _ => Err("Unknown latest source".to_string()),
    }
}

async fn fetch_fallback_version(meta: &CliToolMetadata) -> Result<String, String> {
    let fallback_url = meta
        .fallback_url
        .ok_or_else(|| "No fallback source configured".to_string())?;
    let client = build_client()?;
    fetch_npm_version(&client, fallback_url).await
}

async fn fetch_latest_version(meta: &CliToolMetadata) -> Result<LatestVersionResult, String> {
    let primary = fetch_primary_version(meta).await;
    let fallback = if primary.is_err()
        && matches!(meta.latest_source, "claude_release" | "github_release")
    {
        Some(fetch_fallback_version(meta).await)
    } else {
        None
    };
    select_latest_version_result(meta, primary, fallback)
}

async fn fetch_claude_release_version(
    client: &reqwest::Client,
    url: &str,
) -> Result<String, String> {
    let resp = client
        .get(url)
        .timeout(VERSION_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    crate::cli_probe::extract_semver(&text)
        .ok_or_else(|| "No semver found in Claude release response".to_string())
}

async fn fetch_github_version(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .timeout(VERSION_FETCH_TIMEOUT)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "OneSpace CLI update checker")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    parse_github_release_version(&json)
}

/// Internal result of fetching the latest version, capturing the actual success source.
#[derive(Debug, Clone, PartialEq)]
struct LatestVersionResult {
    version: String,
    source: String,
    url: String,
}

fn normalize_remote_version(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let without_v = trimmed.strip_prefix('v').unwrap_or(trimmed);
    if parse_semver_parts(without_v).is_some() {
        return Ok(without_v.to_string());
    }
    crate::cli_probe::extract_semver(without_v)
        .ok_or_else(|| "No semver found in remote version".to_string())
}

fn parse_github_release_version(json: &serde_json::Value) -> Result<String, String> {
    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "No tag_name in GitHub release response".to_string())?;
    normalize_remote_version(tag)
}

fn parse_npm_latest_version(json: &serde_json::Value) -> Result<String, String> {
    let ver = json
        .get("version")
        .and_then(|v| v.as_str())
        .or_else(|| {
            json.get("dist-tags")
                .and_then(|tags| tags.get("latest"))
                .and_then(|v| v.as_str())
        })
        .ok_or_else(|| "No version field in npm response".to_string())?;
    normalize_remote_version(ver)
}

fn select_latest_version_result(
    meta: &CliToolMetadata,
    primary_result: Result<String, String>,
    fallback_result: Option<Result<String, String>>,
) -> Result<LatestVersionResult, String> {
    match primary_result {
        Ok(version) => Ok(LatestVersionResult {
            version,
            source: meta.latest_source.to_string(),
            url: meta.latest_url.to_string(),
        }),
        Err(primary_err) => {
            if let (Some(fallback_src), Some(fallback_url), Some(fallback_fn)) =
                (meta.fallback_source, meta.fallback_url, fallback_result)
            {
                match fallback_fn {
                    Ok(version) => Ok(LatestVersionResult {
                        version,
                        source: fallback_src.to_string(),
                        url: fallback_url.to_string(),
                    }),
                    Err(fallback_err) => Err(format!(
                        "Failed to fetch latest version from {}: {}; fallback {} failed: {}",
                        meta.latest_source, primary_err, fallback_src, fallback_err
                    )),
                }
            } else {
                Err(format!(
                    "Failed to fetch latest version from {}: {}",
                    meta.latest_source, primary_err
                ))
            }
        }
    }
}
enum SemverParts {
    Stable(i32, i32, i32),
    PreRelease(i32, i32, i32),
}

fn parse_semver_parts(version: &str) -> Option<SemverParts> {
    let without_build = version.split('+').next().unwrap_or(version);
    let parts: Vec<&str> = without_build.splitn(2, '-').collect();
    let core = parts[0];
    let segments: Vec<&str> = core.split('.').take(3).collect();
    let major: i32 = segments.get(0).and_then(|s| s.parse().ok())?;
    let minor: i32 = segments.get(1).and_then(|s| s.parse().ok())?;
    let patch: i32 = segments.get(2).and_then(|s| s.parse().ok())?;
    if let Some(pre) = parts.get(1) {
        if pre.is_empty() {
            return None;
        }
        Some(SemverParts::PreRelease(major, minor, patch))
    } else {
        Some(SemverParts::Stable(major, minor, patch))
    }
}

/// Compare two semver strings. Returns:
/// - `update_available` if latest > current
/// - `current` if latest == current
/// - `unknown_current` if current cannot be parsed
/// - `unknown_latest` if latest cannot be parsed
fn compare_semver(current: &str, latest: &str) -> (bool, String) {
    let cur = match parse_semver_parts(current) {
        Some(v) => v,
        None => return (false, "unknown_current".to_string()),
    };
    let lat = match parse_semver_parts(latest) {
        Some(v) => v,
        None => return (false, "unknown_latest".to_string()),
    };

    let cmp = match (&cur, &lat) {
        (SemverParts::Stable(cm, cn, cp), SemverParts::Stable(lm, ln, lp)) => {
            compare_core((*cm, *cn, *cp), (*lm, *ln, *lp))
        }
        (SemverParts::Stable(cm, cn, cp), SemverParts::PreRelease(lm, ln, lp)) => {
            let core_cmp = compare_core((*cm, *cn, *cp), (*lm, *ln, *lp));
            if core_cmp != 0 {
                core_cmp
            } else {
                1
            } // stable > pre-release at same core
        }
        (SemverParts::PreRelease(cm, cn, cp), SemverParts::Stable(lm, ln, lp)) => {
            let core_cmp = compare_core((*cm, *cn, *cp), (*lm, *ln, *lp));
            if core_cmp != 0 {
                core_cmp
            } else {
                -1
            } // pre-release < stable at same core
        }
        (SemverParts::PreRelease(cm, cn, cp), SemverParts::PreRelease(lm, ln, lp)) => {
            compare_core((*cm, *cn, *cp), (*lm, *ln, *lp))
        }
    };

    match cmp.cmp(&0) {
        std::cmp::Ordering::Less => (true, "update_available".to_string()),
        std::cmp::Ordering::Equal => (false, "current".to_string()),
        std::cmp::Ordering::Greater => (false, "current".to_string()),
    }
}

fn compare_core(cur: (i32, i32, i32), lat: (i32, i32, i32)) -> i32 {
    if cur.0 != lat.0 {
        return cur.0 - lat.0;
    }
    if cur.1 != lat.1 {
        return cur.1 - lat.1;
    }
    cur.2 - lat.2
}

#[tauri::command]
pub async fn check_cli_update(tool: String) -> Result<CliUpdateInfo, String> {
    validate_tool(&tool)?;
    let meta = get_tool_metadata(&tool).unwrap();

    let cmd_name = meta.cmd_name.to_string();
    let probe = tokio::task::spawn_blocking(move || crate::cli_probe::probe_cli_version(&cmd_name))
        .await
        .map_err(|e| format!("probe task failed: {}", e))?;
    if !probe.installed {
        return Ok(CliUpdateInfo {
            tool: tool.clone(),
            installed: false,
            current_version: String::new(),
            current_version_normalized: None,
            latest_version: None,
            latest_source: meta.latest_source.to_string(),
            latest_url: meta.latest_url.to_string(),
            update_available: false,
            compare_status: "not_installed".to_string(),
            update_command: meta.update_command.to_string(),
            error: None,
        });
    }

    let current_raw = probe.version;
    let current_normalized = crate::cli_probe::extract_semver(&current_raw);

    let (latest_version, latest_source, latest_url, compare_status, update_available, error) =
        match fetch_latest_version(&meta).await {
            Ok(result) => {
                let cur_str = current_normalized.as_deref().unwrap_or(&current_raw);
                let (available, status) = compare_semver(cur_str, &result.version);
                (
                    Some(result.version),
                    result.source,
                    result.url,
                    status,
                    available,
                    None,
                )
            }
            Err(e) => (
                None,
                meta.latest_source.to_string(),
                meta.latest_url.to_string(),
                "fetch_failed".to_string(),
                false,
                Some(e),
            ),
        };

    Ok(CliUpdateInfo {
        tool: tool.clone(),
        installed: true,
        current_version: current_raw,
        current_version_normalized: current_normalized,
        latest_version,
        latest_source,
        latest_url,
        update_available,
        compare_status,
        update_command: meta.update_command.to_string(),
        error,
    })
}

#[tauri::command]
pub async fn apply_cli_update(tool: String) -> Result<CliUpdateApplyResult, String> {
    validate_tool(&tool)?;
    let update_command = get_update_command(&tool)?.to_string();

    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let working_dir = home_dir.to_string_lossy().to_string();

    let terminal_app = crate::ai_sessions::resolve_terminal_app_name();

    let result = crate::ai_sessions::run_native_terminal_command_for_update(
        &terminal_app,
        &working_dir,
        &update_command,
    );

    match result {
        Ok(()) => Ok(CliUpdateApplyResult {
            tool,
            success: true,
            terminal_launched: true,
            error: None,
        }),
        Err(e) => Ok(CliUpdateApplyResult {
            tool,
            success: false,
            terminal_launched: false,
            error: Some(e),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compare_semver, fetch_latest_version, get_tool_metadata, normalize_remote_version, parse_github_release_version,
        parse_npm_latest_version, select_latest_version_result, validate_tool, CliToolMetadata,
    };

    #[test]
    fn test_update_commands_match_spec() {
        let claude = get_tool_metadata("claude").unwrap();
        assert_eq!(
            claude.update_command,
            "curl -fsSL https://claude.ai/install.sh | bash"
        );
        let codex = get_tool_metadata("codex").unwrap();
        assert_eq!(codex.update_command, "codex update");
        let antigravity = get_tool_metadata("antigravity").unwrap();
        assert_eq!(
            antigravity.update_command,
            "curl -fsSL https://antigravity.google/cli/install.sh | bash"
        );
        let opencode = get_tool_metadata("opencode").unwrap();
        assert_eq!(
            opencode.update_command,
            "curl -fsSL https://opencode.ai/install | bash"
        );
    }

    #[test]
    fn test_antigravity_metadata_targets_agy_and_official_installer() {
        let meta = get_tool_metadata("antigravity").unwrap();
        assert_eq!(meta.tool, "antigravity");
        assert_eq!(meta.cmd_name, "agy");
        assert_eq!(meta.latest_source, "official_installer");
        assert_eq!(
            meta.latest_url,
            "https://antigravity.google/cli/install.sh"
        );
        assert!(!meta.update_command.contains("npm"));
        assert!(!meta.update_command.contains("gemini-cli"));
        assert!(get_tool_metadata("gemini").is_none());
    }

    #[test]
    fn test_validate_tool_rejects_unknown() {
        assert!(validate_tool("claude").is_ok());
        assert!(validate_tool("codex").is_ok());
        assert!(validate_tool("antigravity").is_ok());
        assert!(validate_tool("opencode").is_ok());
        assert!(validate_tool("gemini").is_err());
        assert!(validate_tool("random_tool").is_err());
    }

    #[test]
    fn test_compare_semver_update_available() {
        let (available, status) = compare_semver("1.2.3", "1.2.4");
        assert!(available);
        assert_eq!(status, "update_available");
    }

    #[test]
    fn test_compare_semver_minor_update() {
        let (available, _) = compare_semver("1.2.9", "1.3.0");
        assert!(available);
    }

    #[test]
    fn test_compare_semver_major_update() {
        let (available, _) = compare_semver("1.9.9", "2.0.0");
        assert!(available);
    }

    #[test]
    fn test_compare_semver_equal() {
        let (available, status) = compare_semver("1.2.3", "1.2.3");
        assert!(!available);
        assert_eq!(status, "current");
    }

    #[test]
    fn test_compare_semver_stable_gt_prerelease() {
        let (available, _) = compare_semver("1.2.3", "1.2.3-beta.1");
        assert!(!available); // current (stable) >= latest (pre-release), no update
    }

    #[test]
    fn test_compare_semver_prerelease_lt_stable() {
        let (available, _) = compare_semver("1.2.3-beta.1", "1.2.3");
        assert!(available); // pre-release < stable, update available
    }

    #[test]
    fn test_compare_semver_unknown_current() {
        let (available, status) = compare_semver("not-a-version", "1.2.3");
        assert!(!available);
        assert_eq!(status, "unknown_current");
    }

    #[test]
    fn test_compare_semver_unknown_latest() {
        let (available, status) = compare_semver("1.2.3", "not-a-version");
        assert!(!available);
        assert_eq!(status, "unknown_latest");
    }

    #[test]
    fn test_opencode_latest_metadata_has_npm_fallback() {
        let meta = get_tool_metadata("opencode").unwrap();
        assert_eq!(meta.latest_source, "github_release");
        assert_eq!(
            meta.latest_url,
            "https://api.github.com/repos/anomalyco/opencode/releases/latest"
        );
        assert_eq!(meta.fallback_source, Some("npm_registry"));
        assert_eq!(
            meta.fallback_url,
            Some("https://registry.npmjs.org/opencode-ai/latest")
        );
    }

    #[test]
    fn test_opencode_latest_parse_github_release_tag_name() {
        let v = parse_github_release_version(&serde_json::json!({"tag_name": "v1.2.3"}));
        assert_eq!(v, Ok("1.2.3".to_string()));

        let v = parse_github_release_version(&serde_json::json!({"tag_name": "opencode-v1.2.4"}));
        assert_eq!(v, Ok("1.2.4".to_string()));

        let v = parse_github_release_version(&serde_json::json!({"tag_name": "v1.2.5-beta.1"}));
        assert_eq!(v, Ok("1.2.5-beta.1".to_string()));

        let v = parse_github_release_version(&serde_json::json!({}));
        assert!(v.is_err());
        assert!(v.unwrap_err().contains("No tag_name"));
    }

    #[test]
    fn test_opencode_latest_parse_npm_version() {
        let v = parse_npm_latest_version(&serde_json::json!({"version": "1.3.0"}));
        assert_eq!(v, Ok("1.3.0".to_string()));

        let v = parse_npm_latest_version(&serde_json::json!({}));
        assert!(v.is_err());
        assert!(v.unwrap_err().contains("No version field"));
    }

    #[test]
    fn test_opencode_latest_selects_npm_fallback_after_github_failure() {
        let meta = get_tool_metadata("opencode").unwrap();
        let primary: Result<String, String> =
            Err("No tag_name in GitHub release response".to_string());
        let fallback: Result<String, String> = Ok("1.3.0".to_string());

        let result = select_latest_version_result(&meta, primary, Some(fallback)).unwrap();
        assert_eq!(result.version, "1.3.0");
        assert_eq!(result.source, "npm_registry");
        assert_eq!(result.url, "https://registry.npmjs.org/opencode-ai/latest");
    }

    #[test]
    fn test_normalize_remote_version_strips_v_prefix() {
        assert_eq!(normalize_remote_version("v1.2.3"), Ok("1.2.3".to_string()));
        assert_eq!(normalize_remote_version("1.2.3"), Ok("1.2.3".to_string()));
        assert_eq!(
            normalize_remote_version("v1.2.5-beta.1"),
            Ok("1.2.5-beta.1".to_string())
        );
    }

    #[test]
    fn test_normalize_remote_version_accepts_build_metadata() {
        assert_eq!(
            normalize_remote_version("v2.0.0-beta.10+build.7"),
            Ok("2.0.0-beta.10+build.7".to_string())
        );
    }

    #[test]
    fn test_compare_semver_ignores_build_metadata() {
        assert_eq!(
            compare_semver("2.0.0+build.7", "2.0.0+build.9"),
            (false, "current".to_string())
        );
        assert_eq!(
            compare_semver("1.9.9+ci.1", "2.0.0+release.3"),
            (true, "update_available".to_string())
        );
    }

    #[test]
    fn test_select_latest_version_result_both_sources_fail() {
        let meta = get_tool_metadata("opencode").unwrap();
        let primary: Result<String, String> =
            Err("No tag_name in GitHub release response".to_string());
        let fallback: Result<String, String> = Err("No version field in npm response".to_string());

        let result = select_latest_version_result(&meta, primary, Some(fallback));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("github_release"));
        assert!(err.contains("npm_registry failed"));
    }

    #[test]
    fn test_select_latest_version_result_primary_success() {
        let meta = get_tool_metadata("opencode").unwrap();
        let primary = Ok("2.0.0".to_string());
        let result = select_latest_version_result(&meta, primary, None).unwrap();
        assert_eq!(result.version, "2.0.0");
        assert_eq!(result.source, "github_release");
    }

    #[test]
    fn test_select_latest_version_result_no_fallback_failure() {
        let meta = get_tool_metadata("codex").unwrap();
        let primary: Result<String, String> = Err("network error".to_string());
        let result = select_latest_version_result(&meta, primary, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("npm_registry"));
    }

    #[tokio::test]
    async fn cli_updates_proxy_switch_primary_and_fallback() {
        let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let pa = crate::proxy::test_support::HttpProxyRecorder::new_with_status("", 500).await;
        let pb = crate::proxy::test_support::HttpProxyRecorder::new("").await;
        pb.set_response(200, r#"{"dist-tags":{"latest":"9.9.9"}}"#.to_string(), vec![]);

        let mgr = crate::proxy::PROXY_MANAGER.get_or_init(|| std::sync::Arc::new(crate::proxy::ProxyManager::new()));
        let a_port: u16 = pa.url().split(':').last().unwrap().parse().unwrap();
        let b_port: u16 = pb.url().split(':').last().unwrap().parse().unwrap();
        // We'll just call cfg via crate? not public. Build ProxyConfig manually.
        use crate::config::ProxyConfig;
        let a_cfg = ProxyConfig { proxy_enabled: true, proxy_type: "http".into(), proxy_host: "127.0.0.1".into(), proxy_port: a_port, proxy_username: None, proxy_password: None, check_interval: 15 };
        mgr.publish(mgr.prepare(Some(a_cfg)).unwrap());

        let published = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let published2 = published.clone();
        let mgr2 = mgr.clone();
        let b_port2 = b_port;
        let cb: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
            if !published2.swap(true, std::sync::atomic::Ordering::SeqCst) {
                let b_cfg = ProxyConfig { proxy_enabled: true, proxy_type: "http".into(), proxy_host: "127.0.0.1".into(), proxy_port: b_port2, proxy_username: None, proxy_password: None, check_interval: 15 };
                mgr2.publish(mgr2.prepare(Some(b_cfg)).unwrap());
            }
        });
        pa.set_on_hit(cb);

        let meta = CliToolMetadata {
            tool: "claude",
            cmd_name: "claude",
            latest_source: "claude_release",
            latest_url: "http://cli-primary.example.test/latest",
            fallback_source: Some("npm_registry"),
            fallback_url: Some("http://cli-fallback.example.test/latest"),
            update_command: "x",
        };
        // primary body should contain semver to succeed? We want failure: pa returns 500, so primary errors.
        // pa body empty + 500 -> error. fallback pb npm shape -> 9.9.9.
        let result = fetch_latest_version(&meta).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        // returned source is fallback source
        assert_eq!(r.source, "npm_registry");
        assert_eq!(pa.targets(), vec!["http://cli-primary.example.test/latest".to_string()]);
        assert_eq!(pb.targets(), vec!["http://cli-fallback.example.test/latest".to_string()]);
    }
}
