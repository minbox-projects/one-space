use regex::Regex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;
use uuid::Uuid;

use super::capabilities::enforce_declared_capabilities;
use super::types::{
    AgentMatrixRow, ModelEffort, ProfileMatrix, ProfileSummary, SUPPORTED_ROLES, SUPPORTED_TOOLS,
    VALID_EFFORTS,
};

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

pub(super) fn resolve_home_dir(home_override: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(home) = home_override {
        Ok(home.to_path_buf())
    } else if let Some(home) = dirs::home_dir() {
        Ok(home)
    } else {
        Err("Failed to resolve home directory".to_string())
    }
}

pub(super) fn write_file_atomic(path: &Path, content: &[u8]) -> Result<(), String> {
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

pub(super) fn read_optional_config(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Failed to read {}: {}", path.display(), e)),
    }
}

fn read_yaml_string(value: &serde_yaml::Value, key: &str, path: &Path) -> Result<String, String> {
    if !value.is_null() && !value.is_mapping() {
        return Err(format!(
            "Invalid YAML in {}: root must be a mapping",
            path.display()
        ));
    }
    match value.get(key) {
        None | Some(serde_yaml::Value::Null) => Ok(String::new()),
        Some(serde_yaml::Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(format!(
            "Invalid {} in {}: expected a string",
            key,
            path.display()
        )),
    }
}

fn read_installed_model(path: &Path, effort_key: &str) -> Result<Option<ModelEffort>, String> {
    let Some(content) = read_optional_config(path)? else {
        return Ok(None);
    };
    let mut result = if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        let doc = content
            .parse::<DocumentMut>()
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;
        let field = |key: &str| -> Result<String, String> {
            match doc.get(key) {
                None => Ok(String::new()),
                Some(value) => value.as_str().map(str::to_string).ok_or_else(|| {
                    format!("Invalid {} in {}: expected a string", key, path.display())
                }),
            }
        };
        ModelEffort {
            model: field("model")?,
            reasoning_effort: field(effort_key)?,
        }
    } else {
        let mut lines = content.lines();
        if lines.next().map(str::trim) != Some("---") {
            return Err(format!("Missing YAML frontmatter in {}", path.display()));
        }
        let mut frontmatter = String::new();
        let mut closed = false;
        for line in lines {
            if line.trim() == "---" {
                closed = true;
                break;
            }
            frontmatter.push_str(line);
            frontmatter.push('\n');
        }
        if !closed {
            return Err(format!("Unclosed YAML frontmatter in {}", path.display()));
        }
        let value = serde_yaml::from_str::<serde_yaml::Value>(&frontmatter)
            .map_err(|e| format!("Failed to parse frontmatter in {}: {}", path.display(), e))?;
        ModelEffort {
            model: read_yaml_string(&value, "model", path)?,
            reasoning_effort: read_yaml_string(&value, effort_key, path)?,
        }
    };
    for field in [&mut result.model, &mut result.reasoning_effort] {
        if field.trim().is_empty() {
            field.clear();
        }
    }
    Ok(Some(result))
}

/// Reads the active profile's installed host models, never its saved profile YAML.
/// Missing activation returns None; existing unreadable or malformed files return an error.
pub fn get_active_models(home_override: Option<&Path>) -> Result<Option<ProfileMatrix>, String> {
    let home = resolve_home_dir(home_override)?;
    let config_path = home.join(".config/ai-workflow/config.yaml");
    let Some(content) = read_optional_config(&config_path)? else {
        return Ok(None);
    };
    let config = serde_yaml::from_str::<serde_yaml::Value>(&content)
        .map_err(|e| format!("Failed to parse {}: {}", config_path.display(), e))?;
    let name = read_yaml_string(&config, "active_profile", &config_path)?
        .trim()
        .to_string();
    if name.is_empty() {
        return Ok(None);
    }

    let mut rows = Vec::with_capacity(SUPPORTED_ROLES.len());
    for &role in &SUPPORTED_ROLES {
        rows.push(AgentMatrixRow {
            role: role.to_string(),
            codex: read_installed_model(
                &home.join(".codex/agents").join(format!("{}.toml", role)),
                "model_reasoning_effort",
            )?,
            claude: read_installed_model(
                &home.join(".claude/agents").join(format!("{}.md", role)),
                "effort",
            )?,
            opencode: read_installed_model(
                &home
                    .join(".config/opencode/agents")
                    .join(format!("{}.md", role)),
                "reasoningEffort",
            )?,
        });
    }
    Ok(Some(ProfileMatrix { name, rows }))
}

#[derive(Serialize)]
struct ProfileDoc<'a> {
    version: &'a str,
    agents: BTreeMap<&'a str, BTreeMap<&'a str, BTreeMap<&'a str, &'a str>>>,
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
    enforce_declared_capabilities(&home, matrix)?;
    let profiles_dir = home.join(".config/ai-workflow/profiles");
    fs::create_dir_all(&profiles_dir)
        .map_err(|e| format!("Failed to create profiles directory: {}", e))?;
    let target_file = profiles_dir.join(format!("{}.yaml", name));
    write_file_atomic(&target_file, yaml_str.as_bytes())
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

/// Renames a profile YAML file.
/// If the renamed profile is currently active, updates active_profile in config.yaml.
pub fn rename_profile(
    old_name: &str,
    new_name: &str,
    home_override: Option<&Path>,
) -> Result<(), String> {
    validate_profile_name(old_name)?;
    validate_profile_name(new_name)?;

    if old_name == new_name {
        return Ok(());
    }

    let home = resolve_home_dir(home_override)?;
    let profiles_dir = home.join(".config/ai-workflow/profiles");
    let old_file = profiles_dir.join(format!("{}.yaml", old_name));
    if !old_file.exists() {
        return Err(format!("Profile '{}' does not exist", old_name));
    }

    let new_file = profiles_dir.join(format!("{}.yaml", new_name));
    if new_file.exists() {
        return Err(format!("Profile '{}' already exists", new_name));
    }

    fs::rename(&old_file, &new_file)
        .map_err(|e| format!("Failed to rename profile '{}' to '{}': {}", old_name, new_name, e))?;

    // Check if the renamed profile is active
    let active_name = get_active_profile_name(&home);
    if active_name.as_deref() == Some(old_name) {
        let config_dir = home.join(".config/ai-workflow");
        let config_file = config_dir.join("config.yaml");
        let mut config_val = if config_file.exists() {
            match fs::read_to_string(&config_file) {
                Ok(content) => serde_yaml::from_str::<serde_yaml::Value>(&content)
                    .unwrap_or_else(|_| serde_yaml::Value::Mapping(serde_yaml::Mapping::new())),
                Err(_) => serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
            }
        } else {
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
        };

        if let serde_yaml::Value::Mapping(ref mut map) = config_val {
            map.insert(
                serde_yaml::Value::String("active_profile".to_string()),
                serde_yaml::Value::String(new_name.to_string()),
            );
        } else {
            let mut map = serde_yaml::Mapping::new();
            map.insert(
                serde_yaml::Value::String("active_profile".to_string()),
                serde_yaml::Value::String(new_name.to_string()),
            );
            config_val = serde_yaml::Value::Mapping(map);
        }

        if let Ok(yaml_str) = serde_yaml::to_string(&config_val) {
            let _ = write_file_atomic(&config_file, yaml_str.as_bytes());
        }
    }

    Ok(())
}
