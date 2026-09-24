use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::DocumentMut;
use uuid::Uuid;

#[cfg(test)]
mod tests;

pub const SUPPORTED_ROLES: [&str; 9] = [
    "backend",
    "documentation-maintainer",
    "file-explorer",
    "frontend",
    "git-operator",
    "researcher",
    "spec-review",
    "standards-review",
    "test",
];

pub const SUPPORTED_TOOLS: [&str; 3] = ["codex", "claude", "opencode"];

pub const VALID_EFFORTS: [&str; 6] = ["low", "medium", "high", "xhigh", "max", "ultra"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSummary {
    pub name: String,
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelEffort {
    pub model: String,
    pub reasoning_effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AgentMatrixRow {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<ModelEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<ModelEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<ModelEffort>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProfileMatrix {
    pub name: String,
    pub rows: Vec<AgentMatrixRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ColumnModelSource {
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelSourcesResult {
    pub opencode: ColumnModelSource,
    pub codex: ColumnModelSource,
    pub claude: ColumnModelSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AgentInstallationInfo {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostInstallation {
    pub host: String,
    pub agents_directory: String,
    #[serde(default)]
    pub agents: Vec<AgentInstallationInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProfileActivationReport {
    pub active_profile: String,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub installations: Vec<HostInstallation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Serialize)]
struct ProfileDoc<'a> {
    version: &'a str,
    agents: BTreeMap<&'a str, BTreeMap<&'a str, BTreeMap<&'a str, &'a str>>>,
}

pub fn validate_profile_name(name: &str) -> Result<(), String> {
    let re = Regex::new(r"(?i)^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$").map_err(|e| e.to_string())?;
    if !re.is_match(name) {
        return Err(format!(
            "Invalid profile name '{}': must match /^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$/i",
            name
        ));
    }
    Ok(())
}

fn resolve_home_dir(home_override: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(home) = home_override {
        Ok(home.to_path_buf())
    } else if let Some(home) = dirs::home_dir() {
        Ok(home)
    } else {
        Err("Failed to resolve home directory".to_string())
    }
}

fn write_file_atomic(path: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directory: {}", e))?;
    }
    let tmp_path = path.with_extension(format!("tmp.{}", Uuid::new_v4()));
    fs::write(&tmp_path, content)
        .map_err(|e| format!("Failed to write temporary file: {}", e))?;
    fs::rename(&tmp_path, path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("Failed to rename temporary file: {}", e)
    })
}

fn resolve_ai_workflow_cli(home_override: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(env_path) = env::var_os("AI_WORKFLOW_CLI_PATH") {
        let path = PathBuf::from(env_path);
        if !path.exists() {
            return Err(format!("ai-workflow binary not found: {}", path.display()));
        }
        return Ok(path);
    }

    if let Some(path_os) = crate::cli_probe::augmented_path() {
        for dir in env::split_paths(&path_os) {
            let candidate = dir.join("ai-workflow");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    let mut extra_dirs = vec![
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ];
    if let Some(home) = home_override.map(Path::to_path_buf).or_else(dirs::home_dir) {
        extra_dirs.push(home.join(".local/bin"));
        extra_dirs.push(home.join(".bun/bin"));
        extra_dirs.push(home.join(".cargo/bin"));
    }
    for dir in extra_dirs {
        let candidate = dir.join("ai-workflow");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err("ai-workflow binary not found in PATH".to_string())
}

fn parse_activation_report(raw: &str) -> Result<ProfileActivationReport, String> {
    let trimmed = raw.trim();
    if let Ok(report) = serde_json::from_str::<ProfileActivationReport>(trimmed) {
        return Ok(report);
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start < end {
            if let Ok(report) =
                serde_json::from_str::<ProfileActivationReport>(&trimmed[start..=end])
            {
                return Ok(report);
            }
        }
    }
    Err(format!(
        "Failed to parse profile activation report from CLI output: {}",
        trimmed
    ))
}

fn get_active_profile_name(home: &Path) -> Option<String> {
    let config_path = home.join(".config/ai-workflow/config.yaml");
    if config_path.exists() {
        if let Ok(content) = fs::read_to_string(&config_path) {
            if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                return val
                    .get("active_profile")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string());
            }
        }
    }
    None
}

/// Lists all profile YAML files under `~/.config/ai-workflow/profiles/`.
/// Marks the one matching `active_profile` in `~/.config/ai-workflow/config.yaml`.
/// If a profile YAML is malformed, records its error without crashing.
pub fn list_profiles(home_override: Option<&Path>) -> Result<Vec<ProfileSummary>, String> {
    let home = resolve_home_dir(home_override)?;
    let profiles_dir = home.join(".config/ai-workflow/profiles");
    if !profiles_dir.exists() {
        return Ok(vec![]);
    }

    let active_profile = get_active_profile_name(&home);

    let entries = match fs::read_dir(&profiles_dir) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };

    let mut profiles = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("yaml") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let name = stem.to_string();
                let active = active_profile.as_deref() == Some(&name);
                let mut error = None;
                match fs::read_to_string(&path) {
                    Ok(content) => match serde_yaml::from_str::<serde_yaml::Value>(&content) {
                        Ok(val) => {
                            if !val.is_mapping() {
                                error = Some("YAML root must be a mapping".to_string());
                            }
                        }
                        Err(e) => {
                            error = Some(format!("Failed to parse YAML: {}", e));
                        }
                    },
                    Err(e) => {
                        error = Some(format!("Failed to read file: {}", e));
                    }
                }
                profiles.push(ProfileSummary {
                    name,
                    active,
                    error,
                });
            }
        }
    }

    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(profiles)
}

/// Retrieves the 9-role x 3-tool matrix for the specified profile.
/// Missing roles in the YAML have `None` entries in their row.
pub fn get_profile_matrix(
    name: &str,
    home_override: Option<&Path>,
) -> Result<ProfileMatrix, String> {
    validate_profile_name(name)?;
    let home = resolve_home_dir(home_override)?;
    let file_path = home
        .join(".config/ai-workflow/profiles")
        .join(format!("{}.yaml", name));

    if !file_path.exists() {
        return Err(format!("Profile file not found: {}", file_path.display()));
    }

    let content = fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read profile file: {}", e))?;
    let yaml_val: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|e| format!("Failed to parse profile YAML (corrupt): {}", e))?;

    let agents_map = yaml_val.get("agents").and_then(|a| a.as_mapping());
    let mut rows = Vec::with_capacity(SUPPORTED_ROLES.len());
    for &role in &SUPPORTED_ROLES {
        let role_val = agents_map.and_then(|m| m.get(serde_yaml::Value::String(role.to_string())));
        let extract_tool = |tool_name: &str| -> Option<ModelEffort> {
            let tool_val = role_val
                .and_then(|rv| rv.get(serde_yaml::Value::String(tool_name.to_string())))?;
            let model = tool_val
                .get(serde_yaml::Value::String("model".to_string()))?
                .as_str()?
                .to_string();
            let reasoning_effort = tool_val
                .get(serde_yaml::Value::String("reasoning_effort".to_string()))?
                .as_str()?
                .to_string();
            Some(ModelEffort {
                model,
                reasoning_effort,
            })
        };

        rows.push(AgentMatrixRow {
            role: role.to_string(),
            codex: extract_tool("codex"),
            claude: extract_tool("claude"),
            opencode: extract_tool("opencode"),
        });
    }

    Ok(ProfileMatrix {
        name: name.to_string(),
        rows,
    })
}

/// Gathers model source candidate lists for opencode, codex, and claude.
/// Individual column errors degrade that column while keeping others available.
pub fn get_model_sources(home_override: Option<&Path>) -> Result<ModelSourcesResult, String> {
    let home = resolve_home_dir(home_override)?;

    // 1. OpenCode column
    let mut opencode = ColumnModelSource::default();
    let opencode_path = home.join(".config/opencode/opencode.json");
    if opencode_path.exists() {
        match fs::read_to_string(&opencode_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(val) => {
                    let mut models_vec = Vec::new();
                    if let Some(providers) = val.get("provider").and_then(|p| p.as_object()) {
                        for (provider_name, provider_data) in providers {
                            if let Some(models) =
                                provider_data.get("models").and_then(|m| m.as_object())
                            {
                                for model_name in models.keys() {
                                    models_vec.push(format!("{}/{}", provider_name, model_name));
                                }
                            }
                        }
                    }
                    models_vec.sort();
                    models_vec.dedup();
                    opencode.models = models_vec;
                }
                Err(e) => {
                    opencode.error = Some(format!("Failed to parse opencode.json: {}", e));
                }
            },
            Err(e) => {
                opencode.error = Some(format!("Failed to read opencode.json: {}", e));
            }
        }
    }

    // Read existing profile models for codex and claude
    let profiles_dir = home.join(".config/ai-workflow/profiles");
    let mut profile_codex_models = HashSet::new();
    let mut profile_claude_models = HashSet::new();
    if profiles_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&profiles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("yaml") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                            if let Some(agents) = val.get("agents").and_then(|a| a.as_mapping()) {
                                for (_role_key, role_val) in agents {
                                    if let Some(codex_val) =
                                        role_val.get(serde_yaml::Value::String("codex".to_string()))
                                    {
                                        if let Some(m) = codex_val
                                            .get(serde_yaml::Value::String("model".to_string()))
                                            .and_then(|v| v.as_str())
                                        {
                                            if !m.trim().is_empty() {
                                                profile_codex_models.insert(m.trim().to_string());
                                            }
                                        }
                                    }
                                    if let Some(claude_val) = role_val
                                        .get(serde_yaml::Value::String("claude".to_string()))
                                    {
                                        if let Some(m) = claude_val
                                            .get(serde_yaml::Value::String("model".to_string()))
                                            .and_then(|v| v.as_str())
                                        {
                                            if !m.trim().is_empty() {
                                                profile_claude_models.insert(m.trim().to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Codex column
    let mut codex = ColumnModelSource::default();
    let codex_path = home.join(".codex/config.toml");
    let mut codex_models = profile_codex_models;
    if codex_path.exists() {
        match fs::read_to_string(&codex_path) {
            Ok(content) => match content.parse::<DocumentMut>() {
                Ok(doc) => {
                    if let Some(item) = doc.get("model") {
                        if let Some(s) = item.as_str() {
                            if !s.trim().is_empty() {
                                codex_models.insert(s.trim().to_string());
                            }
                        }
                    }
                    if let Some(profiles_item) = doc.get("profiles") {
                        if let Some(table) = profiles_item.as_table() {
                            for (_name, profile_item) in table.iter() {
                                if let Some(m) =
                                    profile_item.get("model").and_then(|v| v.as_str())
                                {
                                    if !m.trim().is_empty() {
                                        codex_models.insert(m.trim().to_string());
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    codex.error = Some(format!("Failed to parse config.toml: {}", e));
                }
            },
            Err(e) => {
                codex.error = Some(format!("Failed to read config.toml: {}", e));
            }
        }
    }
    let mut codex_list: Vec<String> = codex_models.into_iter().collect();
    codex_list.sort();
    codex.models = codex_list;

    // 3. Claude column
    let mut claude = ColumnModelSource::default();
    let claude_path = home.join(".claude/settings.json");
    let mut claude_models = profile_claude_models;
    if claude_path.exists() {
        match fs::read_to_string(&claude_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(val) => {
                    if let Some(env_obj) = val.get("env").and_then(|e| e.as_object()) {
                        const CLAUDE_ENV_KEYS: [&str; 4] = [
                            "ANTHROPIC_MODEL",
                            "ANTHROPIC_DEFAULT_OPUS_MODEL",
                            "ANTHROPIC_DEFAULT_SONNET_MODEL",
                            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                        ];
                        for key in CLAUDE_ENV_KEYS {
                            if let Some(model_str) = env_obj.get(key).and_then(|v| v.as_str()) {
                                if !model_str.trim().is_empty() {
                                    claude_models.insert(model_str.trim().to_string());
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    claude.error = Some(format!("Failed to parse settings.json: {}", e));
                }
            },
            Err(e) => {
                claude.error = Some(format!("Failed to read settings.json: {}", e));
            }
        }
    }
    let mut claude_list: Vec<String> = claude_models.into_iter().collect();
    claude_list.sort();
    claude.models = claude_list;

    Ok(ModelSourcesResult {
        opencode,
        codex,
        claude,
    })
}

/// Directly activates an existing profile without rewriting its YAML file.
pub fn activate_profile(
    name: &str,
    home_override: Option<&Path>,
) -> Result<ProfileActivationReport, String> {
    validate_profile_name(name)?;
    let cli_path = resolve_ai_workflow_cli(home_override)?;
    let mut cmd = Command::new(&cli_path);
    cmd.args(["profile", "activate"]);
    // GUI launches often miss fnm/nvm/Homebrew paths, so the `ai-workflow`
    // shim (`exec node ...`) would fail with `exec: node: not found`.
    // Reuse the CLI probe's PATH fixup so the child inherits node dirs.
    if let Some(path) = crate::cli_probe::augmented_path() {
        cmd.env("PATH", path);
    }
    if let Some(home) = home_override {
        cmd.arg("--home").arg(home);
        cmd.env("HOME", home);
    }
    cmd.arg(name);

    let output = cmd
        .output()
        .map_err(|e| format!("failed to execute ai-workflow binary: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let err = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!(
                "ai-workflow profile activate failed with status: {:?}",
                output.status.code()
            )
        };
        return Err(err);
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_activation_report(&stdout_str)
}

fn serialize_profile_yaml(name: &str, matrix: &[AgentMatrixRow]) -> Result<String, String> {
    validate_profile_name(name)?;

    for row in matrix {
        for tool_name in SUPPORTED_TOOLS {
            let tool_opt = match tool_name {
                "codex" => &row.codex,
                "claude" => &row.claude,
                "opencode" => &row.opencode,
                _ => &None,
            };
            if let Some(effort) = tool_opt {
                let model = effort.model.trim();
                if model.is_empty() {
                    return Err(format!(
                        "Invalid model for role '{}' and tool '{}': model cannot be blank",
                        row.role, tool_name
                    ));
                }
                let reasoning_effort = effort.reasoning_effort.trim();
                if !VALID_EFFORTS.contains(&reasoning_effort) {
                    return Err(format!(
                        "Invalid reasoning_effort '{}' for role '{}' and tool '{}': must be one of {:?}",
                        reasoning_effort, row.role, tool_name, VALID_EFFORTS
                    ));
                }
            }
        }
    }

    let mut agents_map: BTreeMap<&str, BTreeMap<&str, BTreeMap<&str, &str>>> = BTreeMap::new();
    for row in matrix {
        let mut tools_map: BTreeMap<&str, BTreeMap<&str, &str>> = BTreeMap::new();
        if let Some(ref codex) = row.codex {
            let mut m = BTreeMap::new();
            m.insert("model", codex.model.trim());
            m.insert("reasoning_effort", codex.reasoning_effort.trim());
            tools_map.insert("codex", m);
        }
        if let Some(ref claude) = row.claude {
            let mut m = BTreeMap::new();
            m.insert("model", claude.model.trim());
            m.insert("reasoning_effort", claude.reasoning_effort.trim());
            tools_map.insert("claude", m);
        }
        if let Some(ref opencode) = row.opencode {
            let mut m = BTreeMap::new();
            m.insert("model", opencode.model.trim());
            m.insert("reasoning_effort", opencode.reasoning_effort.trim());
            tools_map.insert("opencode", m);
        }
        if !tools_map.is_empty() {
            agents_map.insert(&row.role, tools_map);
        }
    }

    let doc = ProfileDoc {
        version: "1.0.0",
        agents: agents_map,
    };
    serde_yaml::to_string(&doc).map_err(|e| format!("Failed to serialize YAML: {}", e))
}

/// Validates and atomically saves profile YAML without changing the active profile.
pub fn save_profile(
    name: &str,
    matrix: &[AgentMatrixRow],
    home_override: Option<&Path>,
) -> Result<(), String> {
    let yaml_str = serialize_profile_yaml(name, matrix)?;
    let home = resolve_home_dir(home_override)?;
    let profiles_dir = home.join(".config/ai-workflow/profiles");
    fs::create_dir_all(&profiles_dir)
        .map_err(|e| format!("Failed to create profiles directory: {}", e))?;
    let target_file = profiles_dir.join(format!("{}.yaml", name));
    write_file_atomic(&target_file, yaml_str.as_bytes())
}

/// Validates profile name, schema (version 1.0.0), effort levels,
/// atomically writes the YAML omitting unconfigured roles, then activates the profile.
/// Restores original YAML snapshot if activation fails.
pub fn save_and_activate_profile(
    name: &str,
    matrix: &[AgentMatrixRow],
    home_override: Option<&Path>,
) -> Result<ProfileActivationReport, String> {
    let yaml_str = serialize_profile_yaml(name, matrix)?;

    let home = resolve_home_dir(home_override)?;
    let profiles_dir = home.join(".config/ai-workflow/profiles");
    fs::create_dir_all(&profiles_dir)
        .map_err(|e| format!("Failed to create profiles directory: {}", e))?;
    let target_file = profiles_dir.join(format!("{}.yaml", name));

    let original_bytes = if target_file.exists() {
        Some(
            fs::read(&target_file)
                .map_err(|e| format!("Failed to read existing profile: {}", e))?,
        )
    } else {
        None
    };

    write_file_atomic(&target_file, yaml_str.as_bytes())?;

    match activate_profile(name, home_override) {
        Ok(report) => Ok(report),
        Err(err) => {
            match original_bytes {
                Some(bytes) => {
                    let _ = write_file_atomic(&target_file, &bytes);
                }
                None => {
                    let _ = fs::remove_file(&target_file);
                }
            }
            Err(err)
        }
    }
}

/// Creates a new profile YAML file.
/// If copy_from is specified, copies content from that existing profile.
/// Otherwise creates a valid minimal schema.
pub fn create_profile(
    name: &str,
    copy_from: Option<&str>,
    home_override: Option<&Path>,
) -> Result<(), String> {
    validate_profile_name(name)?;
    let home = resolve_home_dir(home_override)?;
    let profiles_dir = home.join(".config/ai-workflow/profiles");
    fs::create_dir_all(&profiles_dir)
        .map_err(|e| format!("Failed to create profiles directory: {}", e))?;

    let target_file = profiles_dir.join(format!("{}.yaml", name));
    if target_file.exists() {
        return Err(format!("Profile '{}' already exists", name));
    }

    let yaml_content = if let Some(source_name) = copy_from {
        let trimmed_source = source_name.trim();
        if !trimmed_source.is_empty() {
            validate_profile_name(trimmed_source)?;
            let source_file = profiles_dir.join(format!("{}.yaml", trimmed_source));
            if !source_file.exists() {
                return Err(format!("Source profile '{}' does not exist", trimmed_source));
            }
            fs::read_to_string(&source_file)
                .map_err(|e| format!("Failed to read source profile '{}': {}", trimmed_source, e))?
        } else {
            "version: 1.0.0\nagents: {}\n".to_string()
        }
    } else {
        "version: 1.0.0\nagents: {}\n".to_string()
    };

    write_file_atomic(&target_file, yaml_content.as_bytes())
}

/// Deletes a profile YAML file.
/// Cannot delete active profile.
pub fn delete_profile(name: &str, home_override: Option<&Path>) -> Result<(), String> {
    validate_profile_name(name)?;
    let home = resolve_home_dir(home_override)?;
    let active_name = get_active_profile_name(&home);
    if active_name.as_deref() == Some(name) {
        return Err(format!("Cannot delete active profile '{}'", name));
    }

    let profiles_dir = home.join(".config/ai-workflow/profiles");
    let target_file = profiles_dir.join(format!("{}.yaml", name));
    if target_file.exists() {
        fs::remove_file(&target_file)
            .map_err(|e| format!("Failed to delete profile '{}': {}", name, e))?;
    }
    Ok(())
}

// Tauri commands

#[tauri::command]
pub fn ai_workflow_list_profiles(
    home_override: Option<String>,
) -> Result<Vec<ProfileSummary>, String> {
    list_profiles(home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_get_profile_matrix(
    name: String,
    home_override: Option<String>,
) -> Result<ProfileMatrix, String> {
    get_profile_matrix(&name, home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_get_model_sources(
    home_override: Option<String>,
) -> Result<ModelSourcesResult, String> {
    get_model_sources(home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_save_and_activate_profile(
    name: String,
    matrix: Vec<AgentMatrixRow>,
    home_override: Option<String>,
) -> Result<ProfileActivationReport, String> {
    save_and_activate_profile(&name, &matrix, home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_save_profile(
    name: String,
    matrix: Vec<AgentMatrixRow>,
    home_override: Option<String>,
) -> Result<(), String> {
    save_profile(&name, &matrix, home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_activate_profile(
    name: String,
    home_override: Option<String>,
) -> Result<ProfileActivationReport, String> {
    activate_profile(&name, home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_create_profile(
    name: String,
    copy_from: Option<String>,
    home_override: Option<String>,
) -> Result<(), String> {
    create_profile(&name, copy_from.as_deref(), home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_delete_profile(
    name: String,
    home_override: Option<String>,
) -> Result<(), String> {
    delete_profile(&name, home_override.as_deref().map(Path::new))
}
