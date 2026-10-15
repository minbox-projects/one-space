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
pub struct ModelCapability {
    #[serde(default)]
    pub efforts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ColumnModelSource {
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Exact host-local model ID -> declared raw capabilities. An absent key is unknown.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_capabilities: BTreeMap<String, ModelCapability>,
    /// Optional capability-read diagnostic, isolated from the column `error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_warning: Option<String>,
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

fn read_optional_config(path: &Path) -> Result<Option<String>, String> {
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

/// Appends a trimmed raw effort token, skipping blanks and duplicates.
fn push_capability_effort(efforts: &mut Vec<String>, raw: &str) {
    let token = raw.trim();
    if token.is_empty() {
        return;
    }
    if !efforts.iter().any(|existing| existing == token) {
        efforts.push(token.to_string());
    }
}

/// Extracts Codex per-slug raw efforts from `models_cache.json`.
/// A slug with no non-empty declared effort is unknown and omitted.
fn codex_capabilities_from_value(root: &serde_json::Value) -> BTreeMap<String, ModelCapability> {
    let mut capabilities = BTreeMap::new();
    let Some(models) = root.get("models").and_then(|value| value.as_array()) else {
        return capabilities;
    };
    for model in models {
        let Some(slug) = model.get("slug").and_then(|value| value.as_str()) else {
            continue;
        };
        let slug = slug.trim();
        if slug.is_empty() {
            continue;
        }
        let mut efforts = Vec::new();
        if let Some(levels) = model
            .get("supported_reasoning_levels")
            .and_then(|value| value.as_array())
        {
            for level in levels {
                if let Some(effort) = level.get("effort").and_then(|value| value.as_str()) {
                    push_capability_effort(&mut efforts, effort);
                }
            }
        }
        if efforts.is_empty() {
            continue;
        }
        let default_effort = model
            .get("default_reasoning_level")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        capabilities
            .entry(slug.to_string())
            .or_insert(ModelCapability {
                efforts,
                default_effort,
            });
    }
    capabilities
}

/// A model's explicit `variants` declaration, if any.
/// Declared-but-all-disabled yields a known entry with an empty effort list;
/// a variant without a string `reasoningEffort` is not a declaration.
fn opencode_declared_capability(model: &serde_json::Value) -> Option<ModelCapability> {
    let variants = model.get("variants")?.as_object()?;
    let mut efforts = Vec::new();
    let mut declared = false;
    for variant in variants.values() {
        let Some(raw) = variant
            .get("reasoningEffort")
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        declared = true;
        let disabled = variant
            .get("disabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !disabled {
            push_capability_effort(&mut efforts, raw);
        }
    }
    if !declared {
        return None;
    }
    Some(ModelCapability {
        efforts,
        default_effort: None,
    })
}

/// Resolves one provider object from the optional local catalog.
/// A provider may sit under a top-level `providers` wrapper or directly at the
/// root; a missing or non-object provider resolves to None.
fn opencode_catalog_provider<'a>(
    catalog: &'a serde_json::Value,
    provider: &str,
) -> Option<&'a serde_json::Value> {
    catalog
        .get("providers")
        .and_then(|value| value.as_object())
        .and_then(|providers| providers.get(provider))
        .or_else(|| catalog.get(provider))
}

/// A dynamically discoverable self-operated model is free exactly when its
/// declared `input` and `output` costs are both the number zero. A missing or
/// non-object cost, a missing side, a non-zero value or an unparseable value
/// (including a string) is not free. A `status` of `deprecated` excludes it.
fn opencode_catalog_model_is_free(model: &serde_json::Value) -> bool {
    if model.get("status").and_then(|value| value.as_str()) == Some("deprecated") {
        return false;
    }
    let Some(cost) = model.get("cost").and_then(|value| value.as_object()) else {
        return false;
    };
    ["input", "output"]
        .iter()
        .all(|key| cost.get(*key).and_then(|value| value.as_f64()) == Some(0.0))
}

/// Exact provider/model effort metadata from the optional local catalog.
/// Only `type: "effort"` array values supply capabilities.
fn opencode_catalog_capability(
    catalog: &serde_json::Value,
    provider: &str,
    model: &str,
) -> Option<ModelCapability> {
    let provider_value = opencode_catalog_provider(catalog, provider)?;
    let model_value = provider_value.get("models")?.get(model)?;
    let options = model_value.get("reasoning_options")?.as_array()?;
    let mut efforts = Vec::new();
    for option in options {
        let is_effort = option.get("type").and_then(|value| value.as_str()) == Some("effort");
        if !is_effort {
            continue;
        }
        if let Some(values) = option.get("values").and_then(|value| value.as_array()) {
            for value in values {
                if let Some(raw) = value.as_str() {
                    push_capability_effort(&mut efforts, raw);
                }
            }
        }
    }
    if efforts.is_empty() {
        return None;
    }
    Some(ModelCapability {
        efforts,
        default_effort: None,
    })
}

/// Reloads local capabilities for `home` and rejects a populated known model
/// whose selected effort is not among its declared raw efforts.
fn enforce_declared_capabilities(home: &Path, matrix: &[AgentMatrixRow]) -> Result<(), String> {
    let sources = get_model_sources(Some(home))?;
    for row in matrix {
        let columns = [
            ("codex", &sources.codex, &row.codex),
            ("claude", &sources.claude, &row.claude),
            ("opencode", &sources.opencode, &row.opencode),
        ];
        for (host, column, effort) in columns {
            let Some(effort) = effort else {
                continue;
            };
            let model = effort.model.trim();
            if model.is_empty() {
                continue;
            }
            let Some(capability) = column.model_capabilities.get(model) else {
                continue;
            };
            let selected = effort.reasoning_effort.trim();
            if !capability.efforts.iter().any(|declared| declared == selected) {
                return Err(format!(
                    "Unsupported reasoning_effort '{}' for role '{}' and host '{}' model '{}': declared efforts are {:?}",
                    selected, row.role, host, model, capability.efforts
                ));
            }
        }
    }
    Ok(())
}

/// Gathers model source candidate lists for opencode, codex, and claude.
/// Individual column errors degrade that column while keeping others available.
pub fn get_model_sources(home_override: Option<&Path>) -> Result<ModelSourcesResult, String> {
    let home = resolve_home_dir(home_override)?;

    // 1. OpenCode column
    let mut opencode = ColumnModelSource::default();
    // The optional local catalog fills effort metadata for models whose
    // configuration declares no explicit reasoningEffort variant, and also
    // supplies the current free models of the self-operated providers.
    let mut opencode_catalog: Option<serde_json::Value> = None;
    let opencode_catalog_path = home.join(".cache/opencode/models.json");
    match read_optional_config(&opencode_catalog_path) {
        Ok(Some(content)) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(value) => opencode_catalog = Some(value),
            Err(_) => {
                opencode.capability_warning =
                    Some("Failed to parse opencode model catalog".to_string());
            }
        },
        Ok(None) => {}
        Err(_) => {
            opencode.capability_warning =
                Some("Failed to read opencode model catalog".to_string());
        }
    }

    let mut models_vec: Vec<String> = Vec::new();
    let mut capabilities: BTreeMap<String, ModelCapability> = BTreeMap::new();

    let opencode_path = home.join(".config/opencode/opencode.json");
    if opencode_path.exists() {
        match fs::read_to_string(&opencode_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(val) => {
                    if let Some(providers) = val.get("provider").and_then(|p| p.as_object()) {
                        for (provider_name, provider_data) in providers {
                            if let Some(models) =
                                provider_data.get("models").and_then(|m| m.as_object())
                            {
                                for (model_name, model_data) in models {
                                    let composite = format!("{}/{}", provider_name, model_name);
                                    models_vec.push(composite.clone());
                                    let capability =
                                        match opencode_declared_capability(model_data) {
                                            Some(capability) => Some(capability),
                                            None => {
                                                opencode_catalog.as_ref().and_then(|catalog| {
                                                    opencode_catalog_capability(
                                                        catalog,
                                                        provider_name,
                                                        model_name,
                                                    )
                                                })
                                            }
                                        };
                                    if let Some(capability) = capability {
                                        capabilities.insert(composite, capability);
                                    }
                                }
                            }
                        }
                    }
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

    // Dynamically merge the current free models of the two self-operated
    // providers, independent of any opencode.json declaration, so the merge
    // still applies when opencode.json is missing or unreadable. A missing
    // catalog yields no dynamic entries and no fabricated diagnostic.
    if let Some(catalog) = opencode_catalog.as_ref() {
        for provider in ["opencode", "opencode-go"] {
            let Some(models) = opencode_catalog_provider(catalog, provider)
                .and_then(|value| value.get("models"))
                .and_then(|models| models.as_object())
            else {
                continue;
            };
            for (model_name, model_data) in models {
                if !opencode_catalog_model_is_free(model_data) {
                    continue;
                }
                let composite = format!("{}/{}", provider, model_name);
                models_vec.push(composite.clone());
                // A declared model's explicit variants declaration wins; the
                // catalog only fills a capability the declaration left unset.
                if !capabilities.contains_key(&composite) {
                    if let Some(capability) =
                        opencode_catalog_capability(catalog, provider, model_name)
                    {
                        capabilities.insert(composite, capability);
                    }
                }
            }
        }
    }

    models_vec.sort();
    models_vec.dedup();
    opencode.models = models_vec;
    opencode.model_capabilities = capabilities;

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

    let codex_cache_path = home.join(".codex/models_cache.json");
    match read_optional_config(&codex_cache_path) {
        Ok(Some(content)) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(value) => {
                codex.model_capabilities = codex_capabilities_from_value(&value);
            }
            Err(e) => {
                codex.capability_warning =
                    Some(format!("Failed to parse models_cache.json: {}", e));
            }
        },
        Ok(None) => {}
        Err(e) => {
            codex.capability_warning = Some(e);
        }
    }

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
pub fn ai_workflow_get_active_models() -> Result<Option<ProfileMatrix>, String> {
    get_active_models(None)
}

#[tauri::command]
pub fn ai_workflow_get_model_sources(
    home_override: Option<String>,
) -> Result<ModelSourcesResult, String> {
    get_model_sources(home_override.as_deref().map(Path::new))
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

#[tauri::command]
pub fn ai_workflow_rename_profile(
    old_name: String,
    new_name: String,
    home_override: Option<String>,
) -> Result<(), String> {
    rename_profile(&old_name, &new_name, home_override.as_deref().map(Path::new))
}
