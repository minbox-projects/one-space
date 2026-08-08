use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ffi::OsString;
use std::fs::{self, File};
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

const REPOSITORY_URL: &str = "https://github.com/hengboy/ai-work-flow.git";
const MANAGED_DIRECTORY: &str = "ai-work-flow";
const REPOSITORY_DIRECTORY: &str = "repository";
const INSTALL_SCRIPT: &str = "agent-build/install.mjs";
const ENVIRONMENTS_DIRECTORY: &str = "environments";
const ENVIRONMENT_MARKER: &str = ".environment";
const DEFAULT_ENVIRONMENT: &str = "default";
const MANAGED_AGENT_RELATIVE_PATHS: [&str; 3] =
    [".claude/agents", ".codex/agents", ".config/opencode/agents"];
const KNOWN_ROLES: [&str; 14] = [
    "coding",
    "planning",
    "file-explorer",
    "researcher",
    "document-maintainer",
    "planning-writer",
    "task-planner",
    "full-stack-coder",
    "bug-fixer",
    "git-operator",
    "environment-operator",
    "code-reviewer",
    "review-standards",
    "review-spec",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallOperation {
    Install,
    Update,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallState {
    Idle,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStage {
    Preparing,
    Clone,
    VerifyRepository,
    Pull,
    NpmCi,
    Install,
    Validate,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogSource {
    System,
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallLogEntry {
    pub sequence: u64,
    pub timestamp: String,
    pub stage: InstallStage,
    pub source: LogSource,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallStatus {
    pub state: InstallState,
    pub operation: Option<InstallOperation>,
    pub stage: Option<InstallStage>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub version: Option<String>,
    pub error: Option<InstallError>,
}

impl Default for InstallStatus {
    fn default() -> Self {
        Self {
            state: InstallState::Idle,
            operation: None,
            stage: None,
            started_at: None,
            finished_at: None,
            version: None,
            error: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallVersion {
    pub installed: bool,
    pub version: Option<String>,
    pub error: Option<InstallError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelResult {
    pub accepted: bool,
    pub status: InstallStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EnvironmentSummary {
    pub name: String,
    pub current: bool,
    pub valid: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EnvironmentDocument {
    pub name: String,
    pub content: String,
    pub value: Option<Value>,
    pub current: bool,
    pub valid: bool,
    pub validation_error: Option<InstallError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EnvironmentStatus {
    pub current: String,
    pub exists: bool,
    pub valid: bool,
}

#[derive(Clone, Debug)]
struct BackendError {
    code: &'static str,
    message: String,
    cancelled: bool,
    output: Option<ProcessOutput>,
}

impl BackendError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            cancelled: false,
            output: None,
        }
    }

    fn cancelled() -> Self {
        Self {
            code: "cancelled",
            message: "AI Work Flow installation was cancelled".to_string(),
            cancelled: true,
            output: None,
        }
    }

    fn with_output(mut self, output: ProcessOutput) -> Self {
        self.output = Some(output);
        self
    }

    fn public(&self) -> InstallError {
        InstallError {
            code: self.code.to_string(),
            message: self.message.clone(),
        }
    }
}

impl From<BackendError> for InstallError {
    fn from(error: BackendError) -> Self {
        error.public()
    }
}

#[derive(Default)]
struct RuntimeState {
    status: InstallStatus,
    logs: Vec<InstallLogEntry>,
    cancellation: Option<CancellationToken>,
}

impl RuntimeState {
    fn begin(&mut self, operation: InstallOperation) -> Option<CancellationToken> {
        if self.status.state == InstallState::Running {
            return None;
        }
        let cancellation = CancellationToken::new();
        self.status = InstallStatus {
            state: InstallState::Running,
            operation: Some(operation),
            stage: Some(InstallStage::Preparing),
            started_at: Some(now()),
            finished_at: None,
            version: None,
            error: None,
        };
        self.logs.clear();
        self.cancellation = Some(cancellation.clone());
        Some(cancellation)
    }

    fn log(&mut self, stage: InstallStage, source: LogSource, message: impl Into<String>) {
        self.logs.push(InstallLogEntry {
            sequence: self.logs.len() as u64 + 1,
            timestamp: now(),
            stage,
            source,
            message: message.into(),
        });
    }

    fn finish(&mut self, result: Result<Option<String>, BackendError>) {
        self.status.finished_at = Some(now());
        self.cancellation = None;
        match result {
            Ok(version) => {
                self.status.state = InstallState::Succeeded;
                self.status.stage = Some(InstallStage::Complete);
                self.status.version = version;
                self.status.error = None;
            }
            Err(error) => {
                self.status.state = if error.cancelled {
                    InstallState::Cancelled
                } else {
                    InstallState::Failed
                };
                self.status.error = Some(error.public());
            }
        }
    }
}

static RUNTIME: OnceLock<Mutex<RuntimeState>> = OnceLock::new();

fn runtime() -> &'static Mutex<RuntimeState> {
    RUNTIME.get_or_init(|| Mutex::new(RuntimeState::default()))
}

fn locked_runtime() -> std::sync::MutexGuard<'static, RuntimeState> {
    runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn io_error(context: &str, error: std::io::Error) -> BackendError {
    BackendError::new("io_error", format!("{context}: {error}"))
}

fn symlink_metadata(path: &Path, context: &str) -> Result<fs::Metadata, BackendError> {
    fs::symlink_metadata(path).map_err(|error| io_error(context, error))
}

fn require_directory(path: &Path, context: &str) -> Result<(), BackendError> {
    let metadata = symlink_metadata(path, context)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(BackendError::new(
            "unsafe_path",
            format!("{context} must be a regular directory"),
        ));
    }
    Ok(())
}

fn require_file(path: &Path, context: &str) -> Result<(), BackendError> {
    let metadata = symlink_metadata(path, context)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(BackendError::new(
            "unsafe_path",
            format!("{context} must be a regular file"),
        ));
    }
    Ok(())
}

fn ensure_directory(path: &Path, context: &str) -> Result<(), BackendError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                return Err(BackendError::new(
                    "unsafe_path",
                    format!("{context} must be a regular directory"),
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| io_error(context, error))?;
        }
        Err(error) => return Err(io_error(context, error)),
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct EnvironmentPaths {
    home: PathBuf,
    root: PathBuf,
    environments: PathBuf,
    marker: PathBuf,
    agents: Vec<PathBuf>,
}

fn environment_paths() -> Result<EnvironmentPaths, BackendError> {
    let home = dirs::home_dir()
        .ok_or_else(|| BackendError::new("home_unavailable", "Cannot resolve home directory"))?;
    require_directory(&home, "Home directory")?;
    let config = home.join(".config");
    ensure_directory(&config, "User configuration directory")?;
    environment_paths_from_root(config.join(MANAGED_DIRECTORY))
}

fn environment_paths_from_root(root: PathBuf) -> Result<EnvironmentPaths, BackendError> {
    let parent = root.parent().ok_or_else(|| {
        BackendError::new(
            "unsafe_path",
            "AI Work Flow configuration root has no parent",
        )
    })?;
    require_directory(parent, "AI Work Flow configuration parent")?;
    let home = parent
        .parent()
        .ok_or_else(|| {
            BackendError::new(
                "unsafe_path",
                "AI Work Flow configuration parent has no home directory",
            )
        })?
        .to_path_buf();
    require_directory(&home, "Home directory")?;
    ensure_directory(&root, "AI Work Flow configuration root")?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| io_error("Cannot resolve configuration parent", error))?;
    let canonical_root = fs::canonicalize(&root)
        .map_err(|error| io_error("Cannot resolve AI Work Flow configuration root", error))?;
    if canonical_root.parent() != Some(canonical_parent.as_path()) {
        return Err(BackendError::new(
            "path_outside_config",
            "AI Work Flow configuration root is outside its parent",
        ));
    }
    let environments = root.join(ENVIRONMENTS_DIRECTORY);
    ensure_directory(&environments, "AI Work Flow environments directory")?;
    let canonical_environments = fs::canonicalize(&environments)
        .map_err(|error| io_error("Cannot resolve environments directory", error))?;
    if canonical_environments.parent() != Some(canonical_root.as_path()) {
        return Err(BackendError::new(
            "path_outside_config",
            "AI Work Flow environments directory is outside the configuration root",
        ));
    }
    Ok(EnvironmentPaths {
        home: home.to_path_buf(),
        marker: root.join(ENVIRONMENT_MARKER),
        root,
        environments,
        agents: MANAGED_AGENT_RELATIVE_PATHS
            .iter()
            .map(|relative| home.join(relative))
            .collect(),
    })
}

fn validate_environment_name(name: &str) -> Result<(), BackendError> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || name == "."
        || name == ".."
    {
        return Err(BackendError::new(
            "invalid_environment_name",
            "Environment name must contain 1-64 ASCII letters, digits, dots, underscores, or hyphens",
        ));
    }
    Ok(())
}

fn environment_file(paths: &EnvironmentPaths, name: &str) -> Result<PathBuf, BackendError> {
    validate_environment_name(name)?;
    let path = paths.environments.join(format!("{name}.json"));
    if path.parent() != Some(paths.environments.as_path()) {
        return Err(BackendError::new(
            "path_outside_config",
            "Environment path is outside the environments directory",
        ));
    }
    Ok(path)
}

fn inspect_optional_file(path: &Path, context: &str) -> Result<bool, BackendError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                Err(BackendError::new(
                    "unsafe_path",
                    format!("{context} must be a regular file"),
                ))
            } else {
                Ok(true)
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(context, error)),
    }
}

fn validate_json_values(value: &Value, path: &str) -> Result<(), BackendError> {
    match value {
        Value::String(text)
            if text
                .chars()
                .any(|character| character.is_control() || character == '\u{007f}') =>
        {
            Err(BackendError::new(
                "invalid_environment_config",
                format!("{path} contains a control character"),
            ))
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                validate_json_values(item, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        Value::Object(entries) => {
            for (key, item) in entries {
                validate_json_values(item, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn require_non_empty_string(value: Option<&Value>, path: &str) -> Result<(), BackendError> {
    if !matches!(value, Some(Value::String(value)) if !value.is_empty()) {
        return Err(BackendError::new(
            "invalid_environment_config",
            format!("{path} must be a non-empty string"),
        ));
    }
    Ok(())
}

fn validate_platform_settings(
    role: &str,
    platform: &str,
    settings: &serde_json::Map<String, Value>,
) -> Result<(), BackendError> {
    let allowed_fields: &[&str] = match platform {
        "codex" => &["model", "reasoning"],
        "claude" => &["model", "effort"],
        "opencode" => &["model", "variant", "options"],
        _ => {
            return Err(BackendError::new(
                "invalid_environment_config",
                format!("Unknown platform: {role}.{platform}"),
            ))
        }
    };
    for field in settings.keys() {
        if !allowed_fields.contains(&field.as_str()) {
            return Err(BackendError::new(
                "invalid_environment_config",
                format!("Unknown field: {role}.{platform}.{field}"),
            ));
        }
    }
    match platform {
        "codex" => {
            if let Some(value) = settings.get("model") {
                require_non_empty_string(Some(value), &format!("{role}.codex.model"))?;
            }
            if let Some(value) = settings.get("reasoning") {
                require_non_empty_string(Some(value), &format!("{role}.codex.reasoning"))?;
            }
        }
        "claude" => {
            if let Some(value) = settings.get("model") {
                require_non_empty_string(Some(value), &format!("{role}.claude.model"))?;
            }
            if let Some(value) = settings.get("effort") {
                if !matches!(value.as_str(), Some("low" | "medium" | "high")) {
                    return Err(BackendError::new(
                        "invalid_environment_config",
                        format!("{role}.claude.effort must be low, medium, or high"),
                    ));
                }
            }
        }
        "opencode" => {
            if let Some(value) = settings.get("model") {
                if !matches!(value, Value::Null | Value::String(_)) {
                    return Err(BackendError::new(
                        "invalid_environment_config",
                        format!("{role}.opencode.model must be a string or null"),
                    ));
                }
            }
            if let Some(value) = settings.get("variant") {
                if !matches!(value, Value::Null | Value::String(_)) {
                    return Err(BackendError::new(
                        "invalid_environment_config",
                        format!("{role}.opencode.variant must be a string or null"),
                    ));
                }
            }
            if let Some(value) = settings.get("options") {
                if !value.is_object() {
                    return Err(BackendError::new(
                        "invalid_environment_config",
                        format!("{role}.opencode.options must be an object"),
                    ));
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn validate_environment_value(value: &Value) -> Result<(), BackendError> {
    validate_json_values(value, "configuration")?;
    let object = value.as_object().ok_or_else(|| {
        BackendError::new(
            "invalid_environment_config",
            "Environment configuration must be a JSON object",
        )
    })?;
    let version_is_one = object
        .get("version")
        .and_then(Value::as_f64)
        .is_some_and(|version| version == 1.0);
    if !version_is_one {
        return Err(BackendError::new(
            "invalid_environment_config",
            "Configuration must contain version: 1",
        ));
    }
    let roles = object
        .get("roles")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            BackendError::new(
                "invalid_environment_config",
                "Configuration must contain a roles object",
            )
        })?;
    for key in object.keys() {
        if key != "version" && key != "roles" {
            return Err(BackendError::new(
                "invalid_environment_config",
                format!("Unknown configuration field: {key}"),
            ));
        }
    }
    for (role, role_config) in roles {
        if !KNOWN_ROLES.contains(&role.as_str()) {
            return Err(BackendError::new(
                "invalid_environment_config",
                format!("Unknown role: {role}"),
            ));
        }
        let role_object = role_config.as_object().ok_or_else(|| {
            BackendError::new(
                "invalid_environment_config",
                format!("{role} must be an object"),
            )
        })?;
        for (platform, settings) in role_object {
            let settings = settings.as_object().ok_or_else(|| {
                BackendError::new(
                    "invalid_environment_config",
                    format!("{role}.{platform} must be an object"),
                )
            })?;
            validate_platform_settings(role, platform, settings)?;
        }
    }
    Ok(())
}

fn parse_environment_content(content: &str) -> Result<Value, BackendError> {
    let value: Value = serde_json::from_str(content).map_err(|error| {
        BackendError::new(
            "invalid_environment_json",
            format!("Environment is not valid JSON: {error}"),
        )
    })?;
    validate_environment_value(&value)?;
    Ok(value)
}

fn read_current_environment(paths: &EnvironmentPaths) -> Result<String, BackendError> {
    if !inspect_optional_file(&paths.marker, "AI Work Flow environment marker")? {
        return Ok(DEFAULT_ENVIRONMENT.to_string());
    }
    let content = fs::read_to_string(&paths.marker)
        .map_err(|error| io_error("Cannot read environment marker", error))?;
    let name = content.trim();
    if name.is_empty() || content.chars().any(|character| character.is_control()) {
        return Err(BackendError::new(
            "invalid_environment_marker",
            "Environment marker contains an invalid name",
        ));
    }
    validate_environment_name(name)?;
    Ok(name.to_string())
}

fn atomic_write_regular(path: &Path, content: &[u8]) -> Result<(), BackendError> {
    let parent = path.parent().ok_or_else(|| {
        BackendError::new("unsafe_path", "Atomic write target has no parent directory")
    })?;
    require_directory(parent, "Atomic write parent")?;
    inspect_optional_file(path, "Atomic write target")?;
    let temporary = parent.join(format!(".onespace-write-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| io_error("Cannot create temporary environment file", error))?;
        file.write_all(content)
            .map_err(|error| io_error("Cannot write temporary environment file", error))?;
        file.sync_all()
            .map_err(|error| io_error("Cannot sync temporary environment file", error))?;
        fs::rename(&temporary, path)
            .map_err(|error| io_error("Cannot atomically replace environment file", error))?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_error("Cannot sync environments directory", error))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn environment_document(
    paths: &EnvironmentPaths,
    name: &str,
) -> Result<EnvironmentDocument, BackendError> {
    let path = environment_file(paths, name)?;
    if !inspect_optional_file(&path, "Environment file")? {
        return Err(BackendError::new(
            "environment_not_found",
            format!("Environment '{name}' does not exist"),
        ));
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| io_error("Cannot read environment file", error))?;
    let current = read_current_environment(paths)? == name;
    match parse_environment_content(&content) {
        Ok(value) => Ok(EnvironmentDocument {
            name: name.to_string(),
            content,
            value: Some(value),
            current,
            valid: true,
            validation_error: None,
        }),
        Err(error) => Ok(EnvironmentDocument {
            name: name.to_string(),
            content,
            value: None,
            current,
            valid: false,
            validation_error: Some(error.public()),
        }),
    }
}

fn environment_status_from_paths(
    paths: &EnvironmentPaths,
) -> Result<EnvironmentStatus, BackendError> {
    let current = read_current_environment(paths)?;
    if current == DEFAULT_ENVIRONMENT
        && !inspect_optional_file(&paths.marker, "AI Work Flow environment marker")?
    {
        let default_path = environment_file(paths, DEFAULT_ENVIRONMENT)?;
        let exists = inspect_optional_file(&default_path, "Default environment file")?;
        let valid = if exists {
            let content = fs::read_to_string(&default_path)
                .map_err(|error| io_error("Cannot read default environment file", error))?;
            parse_environment_content(&content).is_ok()
        } else {
            true
        };
        return Ok(EnvironmentStatus {
            current,
            exists: true,
            valid,
        });
    }
    let path = environment_file(paths, &current)?;
    let exists = inspect_optional_file(&path, "Current environment file")?;
    let valid = if exists {
        let content = fs::read_to_string(&path)
            .map_err(|error| io_error("Cannot read current environment file", error))?;
        parse_environment_content(&content).is_ok()
    } else {
        false
    };
    Ok(EnvironmentStatus {
        current,
        exists,
        valid,
    })
}

fn environment_list_from_paths(
    paths: &EnvironmentPaths,
) -> Result<Vec<EnvironmentSummary>, BackendError> {
    let current = read_current_environment(paths)?;
    let mut environments = Vec::new();
    for entry in fs::read_dir(&paths.environments)
        .map_err(|error| io_error("Cannot list environments", error))?
    {
        let entry = entry.map_err(|error| io_error("Cannot inspect environment entry", error))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| io_error("Cannot inspect environment entry", error))?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(BackendError::new(
                "unsafe_path",
                "Environment directory contains a non-regular file",
            ));
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| BackendError::new("invalid_environment_name", "Invalid file name"))?;
        validate_environment_name(name)?;
        let content = fs::read_to_string(&path)
            .map_err(|error| io_error("Cannot read environment file", error))?;
        environments.push(EnvironmentSummary {
            name: name.to_string(),
            current: current == name,
            valid: parse_environment_content(&content).is_ok(),
        });
    }
    environments.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(environments)
}

#[derive(Clone, Debug)]
struct ManagedPaths {
    root: PathBuf,
    repository: PathBuf,
}

fn managed_paths() -> Result<ManagedPaths, BackendError> {
    managed_paths_from_app_dir(crate::config::get_app_dir().map_err(|message| {
        BackendError::new(
            "app_data_unavailable",
            format!("Cannot resolve app data: {message}"),
        )
    })?)
}

fn managed_paths_from_app_dir(app_dir: PathBuf) -> Result<ManagedPaths, BackendError> {
    require_directory(&app_dir, "Application data directory")?;
    let canonical_app = fs::canonicalize(&app_dir)
        .map_err(|error| io_error("Cannot resolve application data directory", error))?;
    let root = app_dir.join(MANAGED_DIRECTORY);
    match fs::symlink_metadata(&root) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                return Err(BackendError::new(
                    "unsafe_path",
                    "AI Work Flow managed root must be a regular directory",
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&root)
                .map_err(|error| io_error("Cannot create AI Work Flow managed root", error))?;
        }
        Err(error) => return Err(io_error("Cannot inspect AI Work Flow managed root", error)),
    }
    let canonical_root = fs::canonicalize(&root)
        .map_err(|error| io_error("Cannot resolve AI Work Flow managed root", error))?;
    if canonical_root.parent() != Some(canonical_app.as_path()) {
        return Err(BackendError::new(
            "path_outside_app_data",
            "AI Work Flow managed root is outside application data",
        ));
    }
    Ok(ManagedPaths {
        repository: root.join(REPOSITORY_DIRECTORY),
        root,
    })
}

fn repository_operation(path: &Path) -> Result<InstallOperation, BackendError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                Err(BackendError::new(
                    "unsafe_path",
                    "Managed repository must be a regular directory",
                ))
            } else {
                Ok(InstallOperation::Update)
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(InstallOperation::Install),
        Err(error) => Err(io_error("Cannot inspect managed repository", error)),
    }
}

fn validate_repository(path: &Path) -> Result<(), BackendError> {
    require_directory(path, "Managed repository")?;
    require_directory(&path.join(".git"), "Managed repository .git")?;
    require_directory(&path.join("agent-build"), "Managed repository agent-build")?;
    require_file(
        &path.join("package.json"),
        "Managed repository package.json",
    )?;
    require_file(
        &path.join("package-lock.json"),
        "Managed repository package-lock.json",
    )?;
    require_file(
        &path.join(INSTALL_SCRIPT),
        "Managed repository install script",
    )?;
    Ok(())
}

fn read_version(path: &Path) -> Result<Option<String>, BackendError> {
    validate_repository(path)?;
    let package_path = path.join("package.json");
    let content = fs::read_to_string(&package_path)
        .map_err(|error| io_error("Cannot read managed repository package.json", error))?;
    let package: serde_json::Value = match serde_json::from_str(&content) {
        Ok(package) => package,
        Err(_) => return Ok(None),
    };
    Ok(package
        .get("version")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Executable {
    Git,
    Npm,
    Node,
}

impl Executable {
    fn name(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Npm => "npm",
            Self::Node => "node",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommandSpec {
    executable: Executable,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    stage: InstallStage,
}

impl CommandSpec {
    fn clone_to(destination: &Path) -> Self {
        Self {
            executable: Executable::Git,
            args: vec![
                "clone".to_string(),
                REPOSITORY_URL.to_string(),
                destination.to_string_lossy().into_owned(),
            ],
            cwd: None,
            stage: InstallStage::Clone,
        }
    }

    fn verify(repository: &Path) -> Self {
        Self {
            executable: Executable::Git,
            args: vec!["rev-parse".to_string(), "--is-inside-work-tree".to_string()],
            cwd: Some(repository.to_path_buf()),
            stage: InstallStage::VerifyRepository,
        }
    }

    fn constrain_origin(repository: &Path) -> Self {
        Self {
            executable: Executable::Git,
            args: vec![
                "remote".to_string(),
                "set-url".to_string(),
                "origin".to_string(),
                REPOSITORY_URL.to_string(),
            ],
            cwd: Some(repository.to_path_buf()),
            stage: InstallStage::Pull,
        }
    }

    fn pull(repository: &Path) -> Self {
        Self {
            executable: Executable::Git,
            args: vec![
                "pull".to_string(),
                "--ff-only".to_string(),
                "origin".to_string(),
            ],
            cwd: Some(repository.to_path_buf()),
            stage: InstallStage::Pull,
        }
    }

    fn npm_ci(repository: &Path) -> Self {
        Self {
            executable: Executable::Npm,
            args: vec!["ci".to_string()],
            cwd: Some(repository.to_path_buf()),
            stage: InstallStage::NpmCi,
        }
    }

    fn install(repository: &Path) -> Self {
        Self {
            executable: Executable::Node,
            args: vec![INSTALL_SCRIPT.to_string()],
            cwd: Some(repository.to_path_buf()),
            stage: InstallStage::Install,
        }
    }

    fn validate(repository: &Path) -> Self {
        Self {
            executable: Executable::Node,
            args: vec![INSTALL_SCRIPT.to_string(), "validate".to_string()],
            cwd: Some(repository.to_path_buf()),
            stage: InstallStage::Validate,
        }
    }

    fn environment_use(repository: &Path, name: &str) -> Result<Self, BackendError> {
        validate_environment_name(name)?;
        Ok(Self {
            executable: Executable::Node,
            args: vec![
                INSTALL_SCRIPT.to_string(),
                "env".to_string(),
                "use".to_string(),
                name.to_string(),
            ],
            cwd: Some(repository.to_path_buf()),
            stage: InstallStage::Validate,
        })
    }
}

#[derive(Clone, Debug)]
struct ProcessOutput {
    stdout: String,
    stderr: String,
}

#[cfg(unix)]
unsafe extern "C" {
    fn kill(process: i32, signal: i32) -> i32;
}

#[cfg(unix)]
fn signal_process_group(pid: u32, signal: i32) {
    // 每个受管命令使用独立进程组，负 pid 指向整个进程组。
    unsafe {
        let _ = kill(-(pid as i32), signal);
    }
}

#[cfg(unix)]
async fn terminate_process_tree(
    child: &mut tokio::process::Child,
    pid: u32,
) -> Result<std::process::ExitStatus, BackendError> {
    signal_process_group(pid, 15);
    let status = tokio::select! {
        result = child.wait() => {
            result.map_err(|error| io_error("Cannot reap cancelled process group", error))?
        }
        _ = sleep(Duration::from_millis(250)) => {
            signal_process_group(pid, 9);
            child.wait().await
                .map_err(|error| io_error("Cannot reap killed process group", error))?
        }
    };
    // 直接子进程可能先于后代退出，再杀一次进程组，避免后代越过取消边界存活。
    signal_process_group(pid, 9);
    Ok(status)
}

#[cfg(not(unix))]
async fn terminate_process_tree(
    child: &mut tokio::process::Child,
    _pid: u32,
) -> Result<std::process::ExitStatus, BackendError> {
    child
        .kill()
        .await
        .map_err(|error| io_error("Cannot kill cancelled process", error))?;
    child
        .wait()
        .await
        .map_err(|error| io_error("Cannot reap cancelled process", error))
}

trait ProcessRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        command: &'a CommandSpec,
        cancellation: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<ProcessOutput, BackendError>> + Send + 'a>>;
}

struct SystemProcessRunner;

impl ProcessRunner for SystemProcessRunner {
    fn run<'a>(
        &'a self,
        command: &'a CommandSpec,
        cancellation: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<ProcessOutput, BackendError>> + Send + 'a>> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(BackendError::cancelled());
            }
            if let Some(cwd) = command.cwd.as_deref() {
                require_directory(cwd, "Command working directory")?;
            }
            let mut process = Command::new(command.executable.name());
            process
                .args(&command.args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(unix)]
            process.process_group(0);
            if let Some(cwd) = command.cwd.as_deref() {
                process.current_dir(cwd);
            }
            let mut child = process.spawn().map_err(|error| {
                BackendError::new(
                    "spawn_failed",
                    format!("Cannot start {}: {error}", command.executable.name()),
                )
            })?;
            let child_pid = child.id().ok_or_else(|| {
                BackendError::new("spawn_failed", "Managed child process has no process id")
            })?;
            let mut stdout = child.stdout.take().ok_or_else(|| {
                BackendError::new("spawn_failed", "Child process stdout is unavailable")
            })?;
            let mut stderr = child.stderr.take().ok_or_else(|| {
                BackendError::new("spawn_failed", "Child process stderr is unavailable")
            })?;
            let stdout_task = tokio::spawn(async move {
                let mut bytes = Vec::new();
                stdout.read_to_end(&mut bytes).await.map(|_| bytes)
            });
            let stderr_task = tokio::spawn(async move {
                let mut bytes = Vec::new();
                stderr.read_to_end(&mut bytes).await.map(|_| bytes)
            });
            let (status, was_cancelled) = tokio::select! {
                result = child.wait() => (
                    result.map_err(|error| io_error("Cannot wait for child process", error))?,
                    false,
                ),
                _ = cancellation.cancelled() => {
                    let status = terminate_process_tree(&mut child, child_pid).await?;
                    (status, true)
                }
            };
            let stdout = stdout_task
                .await
                .map_err(|error| {
                    BackendError::new("io_error", format!("Cannot join stdout reader: {error}"))
                })?
                .map_err(|error| io_error("Cannot read child stdout", error))?;
            let stderr = stderr_task
                .await
                .map_err(|error| {
                    BackendError::new("io_error", format!("Cannot join stderr reader: {error}"))
                })?
                .map_err(|error| io_error("Cannot read child stderr", error))?;
            let output = ProcessOutput {
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
            };
            if was_cancelled {
                return Err(BackendError::cancelled().with_output(output));
            }
            if !status.success() {
                return Err(BackendError::new(
                    "command_failed",
                    format!(
                        "{} failed with {}",
                        command.executable.name(),
                        status
                            .code()
                            .map_or_else(|| "no exit code".to_string(), |code| code.to_string())
                    ),
                )
                .with_output(output));
            }
            Ok(output)
        })
    }
}

fn append_output(stage: InstallStage, output: &ProcessOutput) {
    let mut state = locked_runtime();
    if !output.stdout.is_empty() {
        state.log(stage, LogSource::Stdout, output.stdout.clone());
    }
    if !output.stderr.is_empty() {
        state.log(stage, LogSource::Stderr, output.stderr.clone());
    }
}

async fn run_command(
    runner: &dyn ProcessRunner,
    command: &CommandSpec,
    cancellation: &CancellationToken,
) -> Result<ProcessOutput, BackendError> {
    {
        let mut state = locked_runtime();
        state.status.stage = Some(command.stage);
        state.log(
            command.stage,
            LogSource::System,
            format!("Starting {}", command.executable.name()),
        );
    }
    let result = runner.run(command, cancellation.clone()).await;
    match &result {
        Ok(output) => {
            append_output(command.stage, output);
            locked_runtime().log(command.stage, LogSource::System, "Command succeeded");
        }
        Err(error) => {
            if let Some(output) = &error.output {
                append_output(command.stage, output);
            }
            locked_runtime().log(
                command.stage,
                LogSource::System,
                format!("{}: {}", error.code, error.message),
            );
        }
    }
    result
}

fn create_temporary_repository(root: &Path) -> Result<PathBuf, BackendError> {
    for _ in 0..10 {
        let path = root.join(format!(
            "{REPOSITORY_DIRECTORY}.install-{}",
            uuid::Uuid::new_v4()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(io_error(
                    "Cannot create temporary managed repository",
                    error,
                ))
            }
        }
    }
    Err(BackendError::new(
        "temporary_path_unavailable",
        "Cannot reserve a unique temporary managed repository",
    ))
}

fn copy_repository_entry(source: &Path, destination: &Path) -> Result<(), BackendError> {
    let metadata = symlink_metadata(source, "Managed repository entry")?;
    if metadata.file_type().is_symlink() {
        return Err(BackendError::new(
            "unsafe_path",
            "Managed repository must not contain symbolic links",
        ));
    }
    if metadata.file_type().is_dir() {
        fs::create_dir(destination)
            .map_err(|error| io_error("Cannot create staged repository directory", error))?;
        for entry in fs::read_dir(source)
            .map_err(|error| io_error("Cannot read managed repository entry", error))?
        {
            let entry = entry
                .map_err(|error| io_error("Cannot inspect managed repository entry", error))?;
            copy_repository_entry(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }
    if metadata.file_type().is_file() {
        fs::copy(source, destination)
            .map_err(|error| io_error("Cannot copy managed repository file", error))?;
        return Ok(());
    }
    Err(BackendError::new(
        "unsafe_path",
        "Managed repository must contain only regular files and directories",
    ))
}

fn copy_repository(source: &Path, destination: &Path) -> Result<(), BackendError> {
    require_directory(source, "Managed repository")?;
    require_directory(destination, "Staged managed repository")?;
    for entry in
        fs::read_dir(source).map_err(|error| io_error("Cannot read managed repository", error))?
    {
        let entry = entry.map_err(|error| io_error("Cannot inspect managed repository", error))?;
        if entry.file_name() == "node_modules" {
            // npm ci 会在临时树中重建依赖，复制该目录会带入正式检出中的平台链接。
            continue;
        }
        copy_repository_entry(&entry.path(), &destination.join(entry.file_name()))?;
    }
    Ok(())
}

async fn stage_install_repository(
    paths: &ManagedPaths,
    runner: &dyn ProcessRunner,
    cancellation: &CancellationToken,
) -> Result<PathBuf, BackendError> {
    let temporary = create_temporary_repository(&paths.root)?;
    let result = async {
        run_command(runner, &CommandSpec::clone_to(&temporary), cancellation).await?;
        validate_repository(&temporary)?;
        let verify = run_command(runner, &CommandSpec::verify(&temporary), cancellation).await?;
        if verify.stdout.trim() != "true" {
            return Err(BackendError::new(
                "repository_invalid",
                "Clone result is not a Git work tree",
            ));
        }
        if cancellation.is_cancelled() {
            return Err(BackendError::cancelled());
        }
        Ok(temporary.clone())
    }
    .await;
    if result.is_err() {
        match fs::symlink_metadata(&temporary) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
                let _ = fs::remove_dir_all(&temporary);
            }
            _ => {}
        }
    }
    result
}

async fn stage_update_repository(
    paths: &ManagedPaths,
    runner: &dyn ProcessRunner,
    cancellation: &CancellationToken,
) -> Result<PathBuf, BackendError> {
    validate_repository(&paths.repository)?;
    let temporary = create_temporary_repository(&paths.root)?;
    let result = async {
        copy_repository(&paths.repository, &temporary)?;
        validate_repository(&temporary)?;
        let verify = run_command(runner, &CommandSpec::verify(&temporary), cancellation).await?;
        if verify.stdout.trim() != "true" {
            return Err(BackendError::new(
                "repository_invalid",
                "Managed repository is not a Git work tree",
            ));
        }
        run_command(
            runner,
            &CommandSpec::constrain_origin(&temporary),
            cancellation,
        )
        .await?;
        run_command(runner, &CommandSpec::pull(&temporary), cancellation).await?;
        validate_repository(&temporary)?;
        if cancellation.is_cancelled() {
            return Err(BackendError::cancelled());
        }
        Ok(temporary.clone())
    }
    .await;
    if result.is_err() {
        match fs::symlink_metadata(&temporary) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
                let _ = fs::remove_dir_all(&temporary);
            }
            _ => {}
        }
    }
    result
}

fn publish_repository(
    paths: &ManagedPaths,
    temporary: &Path,
    operation: InstallOperation,
) -> Result<(), BackendError> {
    match operation {
        InstallOperation::Install => {
            if fs::symlink_metadata(&paths.repository).is_ok() {
                return Err(BackendError::new(
                    "repository_changed",
                    "Managed repository appeared while installing",
                ));
            }
            fs::rename(temporary, &paths.repository)
                .map_err(|error| io_error("Cannot publish managed repository", error))
        }
        InstallOperation::Update => {
            let backup = paths.root.join(format!(
                "{REPOSITORY_DIRECTORY}.backup-{}",
                uuid::Uuid::new_v4()
            ));
            fs::rename(&paths.repository, &backup)
                .map_err(|error| io_error("Cannot stage managed repository replacement", error))?;
            match fs::rename(temporary, &paths.repository) {
                Ok(()) => {
                    let _ = fs::remove_dir_all(backup);
                    Ok(())
                }
                Err(error) => {
                    let restore = fs::rename(&backup, &paths.repository);
                    if let Err(restore_error) = restore {
                        return Err(BackendError::new(
                            "repository_rollback_failed",
                            format!(
                                "Cannot publish managed repository ({error}) or restore it ({restore_error})"
                            ),
                        ));
                    }
                    Err(io_error("Cannot publish managed repository", error))
                }
            }
        }
    }
}

async fn execute_workflow(
    paths: ManagedPaths,
    operation: InstallOperation,
    runner: Arc<dyn ProcessRunner>,
    cancellation: CancellationToken,
) -> Result<Option<String>, BackendError> {
    locked_runtime().log(
        InstallStage::Preparing,
        LogSource::System,
        format!("Starting {operation:?}"),
    );
    let temporary = match operation {
        InstallOperation::Install => {
            stage_install_repository(&paths, runner.as_ref(), &cancellation).await
        }
        InstallOperation::Update => {
            stage_update_repository(&paths, runner.as_ref(), &cancellation).await
        }
    }?;
    let result = async {
        validate_repository(&temporary)?;
        for command in [
            CommandSpec::npm_ci(&temporary),
            CommandSpec::install(&temporary),
            CommandSpec::validate(&temporary),
        ] {
            run_command(runner.as_ref(), &command, &cancellation).await?;
        }
        let version = read_version(&temporary)?;
        if cancellation.is_cancelled() {
            return Err(BackendError::cancelled());
        }
        publish_repository(&paths, &temporary, operation)?;
        Ok(version)
    }
    .await;
    if result.is_err() {
        match fs::symlink_metadata(&temporary) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
                let _ = fs::remove_dir_all(&temporary);
            }
            _ => {}
        }
    }
    result
}

fn environment_create_from_paths(
    paths: &EnvironmentPaths,
    name: &str,
    content: &str,
) -> Result<EnvironmentDocument, BackendError> {
    parse_environment_content(content)?;
    let path = environment_file(paths, name)?;
    if inspect_optional_file(&path, "Environment file")? {
        return Err(BackendError::new(
            "environment_exists",
            format!("Environment '{name}' already exists"),
        ));
    }
    atomic_write_regular(&path, content.as_bytes())?;
    environment_document(paths, name)
}

fn environment_update_from_paths(
    paths: &EnvironmentPaths,
    name: &str,
    content: &str,
) -> Result<EnvironmentDocument, BackendError> {
    parse_environment_content(content)?;
    let path = environment_file(paths, name)?;
    if !inspect_optional_file(&path, "Environment file")? {
        return Err(BackendError::new(
            "environment_not_found",
            format!("Environment '{name}' does not exist"),
        ));
    }
    atomic_write_regular(&path, content.as_bytes())?;
    environment_document(paths, name)
}

fn remove_environment_marker(paths: &EnvironmentPaths) -> Result<(), BackendError> {
    if inspect_optional_file(&paths.marker, "AI Work Flow environment marker")? {
        fs::remove_file(&paths.marker)
            .map_err(|error| io_error("Cannot remove environment marker", error))?;
        File::open(&paths.root)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_error("Cannot sync configuration root", error))?;
    }
    Ok(())
}

fn environment_delete_from_paths(
    paths: &EnvironmentPaths,
    name: &str,
) -> Result<EnvironmentStatus, BackendError> {
    let path = environment_file(paths, name)?;
    if !inspect_optional_file(&path, "Environment file")? {
        return Err(BackendError::new(
            "environment_not_found",
            format!("Environment '{name}' does not exist"),
        ));
    }
    if read_current_environment(paths)? != name {
        fs::remove_file(path).map_err(|error| io_error("Cannot delete environment", error))?;
        return environment_status_from_paths(paths);
    }

    let staged = paths
        .environments
        .join(format!(".onespace-delete-{}", uuid::Uuid::new_v4()));
    fs::rename(&path, &staged)
        .map_err(|error| io_error("Cannot stage environment deletion", error))?;
    if let Err(error) = remove_environment_marker(paths) {
        let _ = fs::rename(&staged, &path);
        return Err(error);
    }
    fs::remove_file(&staged).map_err(|error| io_error("Cannot delete environment", error))?;
    environment_status_from_paths(paths)
}

fn marker_snapshot(paths: &EnvironmentPaths) -> Result<Option<Vec<u8>>, BackendError> {
    if inspect_optional_file(&paths.marker, "AI Work Flow environment marker")? {
        fs::read(&paths.marker)
            .map(Some)
            .map_err(|error| io_error("Cannot snapshot environment marker", error))
    } else {
        Ok(None)
    }
}

fn restore_marker(paths: &EnvironmentPaths, snapshot: Option<&[u8]>) -> Result<(), BackendError> {
    match snapshot {
        Some(content) => atomic_write_regular(&paths.marker, content),
        None => remove_environment_marker(paths),
    }
}

#[derive(Clone, Debug)]
enum SnapshotEntry {
    Directory(Vec<(OsString, SnapshotEntry)>),
    File(Vec<u8>),
}

#[derive(Clone, Debug)]
struct ManagedAgentsSnapshot {
    entries: Vec<(PathBuf, Option<SnapshotEntry>)>,
}

fn snapshot_entry(path: &Path, context: &str) -> Result<Option<SnapshotEntry>, BackendError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(context, error)),
    };
    if metadata.file_type().is_symlink() {
        return Err(BackendError::new(
            "unsafe_path",
            format!("{context} must not be a symbolic link"),
        ));
    }
    if metadata.file_type().is_file() {
        return fs::read(path)
            .map(SnapshotEntry::File)
            .map(Some)
            .map_err(|error| io_error(context, error));
    }
    if !metadata.file_type().is_dir() {
        return Err(BackendError::new(
            "unsafe_path",
            format!("{context} must be a regular file or directory"),
        ));
    }
    let mut children = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| io_error(context, error))? {
        let entry = entry.map_err(|error| io_error(context, error))?;
        children.push((
            entry.file_name(),
            snapshot_entry(&entry.path(), &format!("{context} entry"))?
                .ok_or_else(|| BackendError::new("io_error", "Snapshot entry disappeared"))?,
        ));
    }
    children.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(Some(SnapshotEntry::Directory(children)))
}

fn validate_agent_path(paths: &EnvironmentPaths, path: &Path) -> Result<(), BackendError> {
    let relative = path.strip_prefix(&paths.home).map_err(|_| {
        BackendError::new(
            "path_outside_home",
            "Managed agents path is outside the home directory",
        )
    })?;
    let mut current = paths.home.clone();
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        current.push(component.as_os_str());
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(io_error("Cannot inspect managed agents path", error)),
        };
        if metadata.file_type().is_symlink()
            || (components.peek().is_some() && !metadata.file_type().is_dir())
        {
            return Err(BackendError::new(
                "unsafe_path",
                "Managed agents path contains a symbolic link or non-directory parent",
            ));
        }
    }
    Ok(())
}

fn snapshot_managed_agents(
    paths: &EnvironmentPaths,
) -> Result<ManagedAgentsSnapshot, BackendError> {
    let entries = paths
        .agents
        .iter()
        .map(|path| {
            validate_agent_path(paths, path)?;
            Ok((path.clone(), snapshot_entry(path, "Managed agents path")?))
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    Ok(ManagedAgentsSnapshot { entries })
}

fn ensure_agent_parent(paths: &EnvironmentPaths, path: &Path) -> Result<(), BackendError> {
    let parent = path
        .parent()
        .ok_or_else(|| BackendError::new("unsafe_path", "Managed agents path has no parent"))?;
    let relative = parent.strip_prefix(&paths.home).map_err(|_| {
        BackendError::new(
            "path_outside_home",
            "Managed agents path is outside the home directory",
        )
    })?;
    let mut current = paths.home.clone();
    for component in relative.components() {
        current.push(component.as_os_str());
        ensure_directory(&current, "Managed agents parent")?;
    }
    Ok(())
}

fn remove_snapshot_target(path: &Path, context: &str) -> Result<(), BackendError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error(context, error)),
    };
    if metadata.file_type().is_symlink() {
        return Err(BackendError::new(
            "unsafe_path",
            format!("{context} must not be a symbolic link"),
        ));
    }
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path).map_err(|error| io_error(context, error))
    } else if metadata.file_type().is_file() {
        fs::remove_file(path).map_err(|error| io_error(context, error))
    } else {
        Err(BackendError::new(
            "unsafe_path",
            format!("{context} must be a regular file or directory"),
        ))
    }
}

fn restore_snapshot_entry(
    paths: &EnvironmentPaths,
    path: &Path,
    entry: &SnapshotEntry,
) -> Result<(), BackendError> {
    ensure_agent_parent(paths, path)?;
    match entry {
        SnapshotEntry::File(content) => atomic_write_regular(path, content),
        SnapshotEntry::Directory(children) => {
            ensure_directory(path, "Managed agents directory")?;
            for (name, child) in children {
                restore_snapshot_entry(paths, &path.join(name), child)?;
            }
            Ok(())
        }
    }
}

fn restore_managed_agents(
    paths: &EnvironmentPaths,
    snapshot: &ManagedAgentsSnapshot,
) -> Result<(), BackendError> {
    for (path, entry) in &snapshot.entries {
        validate_agent_path(paths, path)?;
        remove_snapshot_target(path, "Managed agents target")?;
        if let Some(entry) = entry {
            restore_snapshot_entry(paths, path, entry)?;
        }
    }
    Ok(())
}

fn restore_environment_state(
    paths: &EnvironmentPaths,
    marker: Option<&[u8]>,
    agents: &ManagedAgentsSnapshot,
) -> Result<(), BackendError> {
    let marker_result = restore_marker(paths, marker);
    let agents_result = restore_managed_agents(paths, agents);
    match (marker_result, agents_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(marker), Ok(())) => Err(BackendError::new(
            "rollback_failed",
            format!("Cannot restore environment marker: {}", marker.message),
        )),
        (Ok(()), Err(agents)) => Err(BackendError::new(
            "rollback_failed",
            format!("Cannot restore managed Agents: {}", agents.message),
        )),
        (Err(marker), Err(agents)) => Err(BackendError::new(
            "rollback_failed",
            format!(
                "Cannot restore environment marker or managed Agents: {}; {}",
                marker.message, agents.message
            ),
        )),
    }
}

async fn environment_use_from_paths(
    paths: &EnvironmentPaths,
    repository: &Path,
    name: &str,
    runner: &dyn ProcessRunner,
) -> Result<EnvironmentStatus, BackendError> {
    let document = environment_document(paths, name)?;
    if !document.valid {
        return Err(BackendError::new(
            "invalid_environment_config",
            format!("Environment '{name}' is invalid"),
        ));
    }
    validate_repository(repository)?;
    let command = CommandSpec::environment_use(repository, name)?;
    let marker = marker_snapshot(paths)?;
    let agents = snapshot_managed_agents(paths)?;
    let result = runner.run(&command, CancellationToken::new()).await;
    if let Err(error) = result {
        if let Err(rollback) = restore_environment_state(paths, marker.as_deref(), &agents) {
            return Err(rollback);
        }
        return Err(error);
    }
    let status = match read_current_environment(paths) {
        Ok(current) if current == name => environment_status_from_paths(paths),
        _ => Err(BackendError::new(
            "environment_switch_incomplete",
            "AI Work Flow did not activate the requested environment",
        )),
    };
    match status {
        Ok(status) if status.exists && status.valid => Ok(status),
        Ok(_) | Err(_) => {
            if let Err(rollback) = restore_environment_state(paths, marker.as_deref(), &agents) {
                return Err(rollback);
            }
            Err(BackendError::new(
                "environment_switch_incomplete",
                "AI Work Flow did not activate a valid requested environment",
            ))
        }
    }
}

#[tauri::command]
pub fn ai_work_flow_environment_list() -> Result<Vec<EnvironmentSummary>, InstallError> {
    let paths = environment_paths().map_err(InstallError::from)?;
    environment_list_from_paths(&paths).map_err(InstallError::from)
}

#[tauri::command]
pub fn ai_work_flow_environment_create(
    name: String,
    content: String,
) -> Result<EnvironmentDocument, InstallError> {
    let paths = environment_paths().map_err(InstallError::from)?;
    environment_create_from_paths(&paths, &name, &content).map_err(InstallError::from)
}

#[tauri::command]
pub fn ai_work_flow_environment_read(name: String) -> Result<EnvironmentDocument, InstallError> {
    let paths = environment_paths().map_err(InstallError::from)?;
    environment_document(&paths, &name).map_err(InstallError::from)
}

#[tauri::command]
pub fn ai_work_flow_environment_update(
    name: String,
    content: String,
) -> Result<EnvironmentDocument, InstallError> {
    let paths = environment_paths().map_err(InstallError::from)?;
    environment_update_from_paths(&paths, &name, &content).map_err(InstallError::from)
}

#[tauri::command]
pub fn ai_work_flow_environment_delete(name: String) -> Result<EnvironmentStatus, InstallError> {
    let paths = environment_paths().map_err(InstallError::from)?;
    environment_delete_from_paths(&paths, &name).map_err(InstallError::from)
}

#[tauri::command]
pub async fn ai_work_flow_environment_use(name: String) -> Result<EnvironmentStatus, InstallError> {
    let paths = environment_paths().map_err(InstallError::from)?;
    let managed = managed_paths().map_err(InstallError::from)?;
    environment_use_from_paths(&paths, &managed.repository, &name, &SystemProcessRunner)
        .await
        .map_err(InstallError::from)
}

#[tauri::command]
pub fn ai_work_flow_environment_status() -> Result<EnvironmentStatus, InstallError> {
    let paths = environment_paths().map_err(InstallError::from)?;
    environment_status_from_paths(&paths).map_err(InstallError::from)
}

#[tauri::command]
pub fn ai_work_flow_install_status_get() -> InstallStatus {
    locked_runtime().status.clone()
}

#[tauri::command]
pub fn ai_work_flow_install_version_get() -> InstallVersion {
    match managed_paths() {
        Ok(paths) => install_version_from_paths(&paths),
        Err(error) => InstallVersion {
            installed: false,
            version: None,
            error: Some(error.public()),
        },
    }
}

fn install_version_from_paths(paths: &ManagedPaths) -> InstallVersion {
    match repository_operation(&paths.repository) {
        Ok(InstallOperation::Install) => InstallVersion {
            installed: false,
            version: None,
            error: None,
        },
        Ok(InstallOperation::Update) => match read_version(&paths.repository) {
            Ok(version) => InstallVersion {
                installed: true,
                version,
                error: None,
            },
            Err(error) => InstallVersion {
                installed: true,
                version: None,
                error: Some(error.public()),
            },
        },
        Err(error) => InstallVersion {
            installed: false,
            version: None,
            error: Some(error.public()),
        },
    }
}

#[tauri::command]
pub async fn ai_work_flow_install_or_update() -> InstallStatus {
    let paths = match managed_paths() {
        Ok(paths) => paths,
        Err(error) => {
            let mut state = locked_runtime();
            if state.status.state == InstallState::Running {
                return state.status.clone();
            }
            state.status = InstallStatus {
                state: InstallState::Failed,
                operation: None,
                stage: Some(InstallStage::Preparing),
                started_at: Some(now()),
                finished_at: Some(now()),
                version: None,
                error: Some(error.public()),
            };
            state.logs.clear();
            state.log(InstallStage::Preparing, LogSource::System, error.message);
            return state.status.clone();
        }
    };
    let operation = match repository_operation(&paths.repository) {
        Ok(operation) => operation,
        Err(error) => {
            let mut state = locked_runtime();
            if state.status.state == InstallState::Running {
                return state.status.clone();
            }
            state.status = InstallStatus {
                state: InstallState::Failed,
                operation: None,
                stage: Some(InstallStage::Preparing),
                started_at: Some(now()),
                finished_at: Some(now()),
                version: None,
                error: Some(error.public()),
            };
            state.logs.clear();
            state.log(InstallStage::Preparing, LogSource::System, error.message);
            return state.status.clone();
        }
    };
    let (status, cancellation) = {
        let mut state = locked_runtime();
        let Some(cancellation) = state.begin(operation) else {
            return state.status.clone();
        };
        (state.status.clone(), cancellation)
    };
    tauri::async_runtime::spawn(async move {
        let result = execute_workflow(
            paths,
            operation,
            Arc::new(SystemProcessRunner),
            cancellation,
        )
        .await;
        let mut state = locked_runtime();
        if let Err(error) = &result {
            let stage = state.status.stage.unwrap_or(InstallStage::Preparing);
            state.log(
                stage,
                LogSource::System,
                format!("{}: {}", error.code, error.message),
            );
        }
        state.finish(result);
    });
    status
}

#[tauri::command]
pub fn ai_work_flow_install_cancel() -> CancelResult {
    let state = locked_runtime();
    let accepted = state.status.state == InstallState::Running;
    if accepted {
        if let Some(cancellation) = &state.cancellation {
            cancellation.cancel();
        }
    }
    CancelResult {
        accepted,
        status: state.status.clone(),
    }
}

#[tauri::command]
pub fn ai_work_flow_install_logs_get() -> Vec<InstallLogEntry> {
    locked_runtime().logs.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::ffi::OsString;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::MutexGuard;

    fn temporary_root() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("onespace-ai-work-flow-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temporary root");
        path
    }

    fn create_repository(path: &Path, version: &str) {
        fs::create_dir_all(path.join(".git")).expect("create .git");
        fs::create_dir_all(path.join("agent-build")).expect("create agent-build");
        fs::write(
            path.join("package.json"),
            format!(r#"{{"version":"{version}"}}"#),
        )
        .expect("write package.json");
        fs::write(path.join("package-lock.json"), "{}").expect("write package-lock.json");
        fs::write(path.join(INSTALL_SCRIPT), "export {};").expect("write install script");
    }

    fn valid_environment_content() -> &'static str {
        r#"{"version":1,"roles":{}}"#
    }

    #[derive(Default)]
    struct FakeRunner {
        commands: Mutex<Vec<CommandSpec>>,
        results: Mutex<VecDeque<Result<ProcessOutput, BackendError>>>,
        calls: AtomicUsize,
        cancel_after: Option<InstallStage>,
    }

    impl FakeRunner {
        fn with_results(results: Vec<Result<ProcessOutput, BackendError>>) -> Self {
            Self {
                results: Mutex::new(results.into()),
                ..Self::default()
            }
        }

        fn cancelling_after(stage: InstallStage) -> Self {
            Self {
                cancel_after: Some(stage),
                ..Self::default()
            }
        }
    }

    impl ProcessRunner for FakeRunner {
        fn run<'a>(
            &'a self,
            command: &'a CommandSpec,
            cancellation: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = Result<ProcessOutput, BackendError>> + Send + 'a>>
        {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.commands.lock().unwrap().push(command.clone());
                if cancellation.is_cancelled() {
                    return Err(BackendError::cancelled());
                }
                if command.stage == InstallStage::Clone {
                    let destination = PathBuf::from(command.args.last().unwrap());
                    create_repository(&destination, "1.2.3");
                }
                let result = self.results.lock().unwrap().pop_front().unwrap_or_else(|| {
                    Ok(ProcessOutput {
                        stdout: if command.stage == InstallStage::VerifyRepository {
                            "true\n".to_string()
                        } else {
                            String::new()
                        },
                        stderr: String::new(),
                    })
                });
                if self.cancel_after == Some(command.stage) {
                    cancellation.cancel();
                }
                result
            })
        }
    }

    struct EnvironmentUseRunner {
        marker: PathBuf,
        agents: PathBuf,
        fail: bool,
        mutate_marker_before_failure: bool,
        commands: Mutex<Vec<CommandSpec>>,
        calls: AtomicUsize,
    }

    impl EnvironmentUseRunner {
        fn succeeding(marker: PathBuf, agents: PathBuf) -> Self {
            Self {
                marker,
                agents,
                fail: false,
                mutate_marker_before_failure: false,
                commands: Mutex::new(Vec::new()),
                calls: AtomicUsize::new(0),
            }
        }

        fn failing(marker: PathBuf, agents: PathBuf, mutate_marker_before_failure: bool) -> Self {
            Self {
                marker,
                agents,
                fail: true,
                mutate_marker_before_failure,
                commands: Mutex::new(Vec::new()),
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl ProcessRunner for EnvironmentUseRunner {
        fn run<'a>(
            &'a self,
            command: &'a CommandSpec,
            _cancellation: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = Result<ProcessOutput, BackendError>> + Send + 'a>>
        {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.commands.lock().unwrap().push(command.clone());
                if self.fail {
                    if self.mutate_marker_before_failure {
                        fs::write(&self.marker, "partially-switched")
                            .expect("simulate partial marker write");
                        fs::create_dir_all(&self.agents).expect("create simulated agents");
                        fs::write(self.agents.join("managed.md"), "partially-switched")
                            .expect("simulate partial agents write");
                    }
                    return Err(BackendError::new(
                        "command_failed",
                        "environment use failed",
                    ));
                }
                fs::write(&self.marker, &command.args[3]).expect("activate environment");
                Ok(ProcessOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                })
            })
        }
    }

    fn environment_test_paths() -> (PathBuf, EnvironmentPaths) {
        let temporary = temporary_root();
        let config = temporary.join(".config");
        fs::create_dir(&config).expect("create configuration parent");
        let paths = environment_paths_from_root(config.join(MANAGED_DIRECTORY))
            .expect("create environment paths");
        (temporary, paths)
    }

    struct TemporaryHome {
        path: PathBuf,
        previous: Option<OsString>,
        _lock: MutexGuard<'static, ()>,
    }

    impl TemporaryHome {
        fn new() -> Self {
            let lock = crate::lock_test_home_env();
            let path = temporary_root();
            fs::create_dir_all(path.join(".config")).unwrap();
            let previous = std::env::var_os("HOME");
            std::env::set_var("HOME", &path);
            Self {
                path,
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for TemporaryHome {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(previous) => std::env::set_var("HOME", previous),
                None => std::env::remove_var("HOME"),
            }
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn fixed_commands_have_no_public_url_command_or_argument_inputs() {
        let repository = Path::new("/managed/repository");
        assert_eq!(
            REPOSITORY_URL,
            "https://github.com/hengboy/ai-work-flow.git"
        );
        assert_eq!(
            CommandSpec::clone_to(repository).args,
            ["clone", REPOSITORY_URL, "/managed/repository"]
        );
        assert_eq!(CommandSpec::npm_ci(repository).args, ["ci"]);
        assert_eq!(CommandSpec::install(repository).args, [INSTALL_SCRIPT]);
        assert_eq!(
            CommandSpec::validate(repository).args,
            [INSTALL_SCRIPT, "validate"]
        );
        assert!(matches!(
            CommandSpec::npm_ci(repository).executable,
            Executable::Npm
        ));
        assert!(matches!(
            CommandSpec::install(repository).executable,
            Executable::Node
        ));
        assert_eq!(
            CommandSpec::environment_use(repository, "team-prod")
                .unwrap()
                .args,
            [INSTALL_SCRIPT, "env", "use", "team-prod"]
        );
    }

    #[test]
    fn environment_names_accept_only_the_bounded_ascii_allowlist() {
        for name in ["a", "default", "team.prod_2-a", &"a".repeat(64)] {
            validate_environment_name(name).expect("valid environment name");
        }
        for name in [
            "",
            ".",
            "..",
            "../escape",
            "nested/name",
            "nested\\name",
            "has space",
            "line\nbreak",
            "é",
            &"a".repeat(65),
        ] {
            assert_eq!(
                validate_environment_name(name).unwrap_err().code,
                "invalid_environment_name",
                "{name:?} must be rejected"
            );
        }
    }

    #[test]
    fn environment_paths_reject_symlinks_and_non_regular_entries() {
        let temporary = temporary_root();
        let config = temporary.join(".config");
        fs::create_dir(&config).unwrap();
        let root = config.join(MANAGED_DIRECTORY);
        fs::write(&root, "not a directory").unwrap();
        assert_eq!(
            environment_paths_from_root(root.clone()).unwrap_err().code,
            "unsafe_path"
        );
        fs::remove_file(&root).unwrap();

        #[cfg(unix)]
        {
            let outside = temporary.join("outside");
            fs::create_dir(&outside).unwrap();
            std::os::unix::fs::symlink(&outside, &root).unwrap();
            assert_eq!(
                environment_paths_from_root(root.clone()).unwrap_err().code,
                "unsafe_path"
            );
            fs::remove_file(&root).unwrap();
        }

        let paths = environment_paths_from_root(root).unwrap();
        fs::create_dir(paths.environments.join("directory.json")).unwrap();
        assert_eq!(
            environment_list_from_paths(&paths).unwrap_err().code,
            "unsafe_path"
        );
        fs::remove_dir(paths.environments.join("directory.json")).unwrap();

        #[cfg(unix)]
        {
            let outside = temporary.join("outside.json");
            fs::write(&outside, "{}").unwrap();
            std::os::unix::fs::symlink(&outside, paths.environments.join("linked.json")).unwrap();
            assert_eq!(
                environment_document(&paths, "linked").unwrap_err().code,
                "unsafe_path"
            );
        }
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn environment_create_read_update_preserve_complete_json_atomically() {
        let (temporary, paths) = environment_test_paths();
        let original = "{\n  \"version\": 1,\n  \"roles\": {\n    \"coding\": {\n      \"opencode\": {\"model\": \"provider/model\", \"options\": {\"unknown\": [1, 2]}}\n    }\n  }\n}\n";
        let created = environment_create_from_paths(&paths, "team", original).unwrap();
        assert_eq!(created.content, original);
        assert_eq!(
            created.value.unwrap()["roles"]["coding"]["opencode"]["options"]["unknown"],
            serde_json::json!([1, 2])
        );

        let updated = "{\"version\":1,\"roles\":{\"coding\":{\"opencode\":{\"model\":\"provider/model\",\"variant\":\"medium\",\"options\":{\"extra\":{\"preserved\":true}}}}}}";
        let document = environment_update_from_paths(&paths, "team", updated).unwrap();
        assert_eq!(document.content, updated);
        assert_eq!(
            fs::read_to_string(paths.environments.join("team.json")).unwrap(),
            updated
        );
        assert!(fs::read_dir(&paths.environments)
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".onespace-write-")));
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn environment_invalid_json_or_config_never_changes_files() {
        let (temporary, paths) = environment_test_paths();
        assert_eq!(
            environment_create_from_paths(&paths, "broken", "{not json")
                .unwrap_err()
                .code,
            "invalid_environment_json"
        );
        assert!(!paths.environments.join("broken.json").exists());
        assert_eq!(
            environment_create_from_paths(&paths, "array", "[]")
                .unwrap_err()
                .code,
            "invalid_environment_config"
        );
        assert!(!paths.environments.join("array.json").exists());

        let invalid_rule = r#"{"version":1,"roles":{"coding":{"claude":{"effort":"extreme"}}}}"#;
        assert_eq!(
            environment_create_from_paths(&paths, "rule-invalid", invalid_rule)
                .unwrap_err()
                .code,
            "invalid_environment_config"
        );
        assert!(!paths.environments.join("rule-invalid.json").exists());

        environment_create_from_paths(&paths, "stable", valid_environment_content()).unwrap();
        let before = fs::read(paths.environments.join("stable.json")).unwrap();
        assert_eq!(
            environment_update_from_paths(&paths, "stable", "null")
                .unwrap_err()
                .code,
            "invalid_environment_config"
        );
        assert_eq!(
            fs::read(paths.environments.join("stable.json")).unwrap(),
            before
        );

        let invalid_rule = r#"{"version":1,"roles":{"coding":{"codex":{"reasoning":false}}}}"#;
        fs::write(paths.environments.join("read-invalid.json"), invalid_rule).unwrap();
        let document = environment_document(&paths, "read-invalid").unwrap();
        assert!(!document.valid);
        assert_eq!(
            document.validation_error.as_ref().unwrap().code,
            "invalid_environment_config"
        );
        let list = environment_list_from_paths(&paths).unwrap();
        assert!(
            !list
                .iter()
                .find(|item| item.name == "read-invalid")
                .unwrap()
                .valid
        );
        fs::write(&paths.marker, "read-invalid").unwrap();
        let status = environment_status_from_paths(&paths).unwrap();
        assert_eq!(status.current, "read-invalid");
        assert!(!status.valid);
        fs::remove_file(&paths.marker).unwrap();
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn environment_status_and_delete_use_default_fallback_semantics() {
        let (temporary, paths) = environment_test_paths();
        assert_eq!(
            environment_status_from_paths(&paths).unwrap(),
            EnvironmentStatus {
                current: DEFAULT_ENVIRONMENT.to_string(),
                exists: true,
                valid: true,
            }
        );
        environment_create_from_paths(&paths, "current", valid_environment_content()).unwrap();
        environment_create_from_paths(&paths, "other", valid_environment_content()).unwrap();
        fs::write(&paths.marker, "current").unwrap();

        let status = environment_delete_from_paths(&paths, "other").unwrap();
        assert_eq!(status.current, "current");
        assert_eq!(fs::read_to_string(&paths.marker).unwrap(), "current");
        let status = environment_delete_from_paths(&paths, "current").unwrap();
        assert_eq!(status.current, DEFAULT_ENVIRONMENT);
        assert!(status.exists);
        assert!(status.valid);
        assert!(!paths.marker.exists());
        fs::remove_dir_all(temporary).unwrap();
    }

    #[tokio::test]
    async fn environment_use_validates_before_fixed_command_and_preserves_failure_state() {
        let (temporary, paths) = environment_test_paths();
        environment_create_from_paths(&paths, "old", valid_environment_content()).unwrap();
        environment_create_from_paths(&paths, "next", valid_environment_content()).unwrap();
        fs::write(&paths.marker, "old").unwrap();
        fs::create_dir_all(&paths.agents[0]).unwrap();
        fs::write(paths.agents[0].join("managed.md"), "old-agents").unwrap();
        let repository = temporary.join("repository");
        create_repository(&repository, "1.0.0");
        let onespace_state = temporary.join("onespace-ai-environments.json");
        fs::write(&onespace_state, "{\"unchanged\":true}").unwrap();

        let failing =
            EnvironmentUseRunner::failing(paths.marker.clone(), paths.agents[0].clone(), true);
        assert_eq!(
            environment_use_from_paths(&paths, &repository, "next", &failing)
                .await
                .unwrap_err()
                .code,
            "command_failed"
        );
        assert_eq!(fs::read_to_string(&paths.marker).unwrap(), "old");
        assert_eq!(
            fs::read_to_string(paths.agents[0].join("managed.md")).unwrap(),
            "old-agents"
        );
        assert_eq!(
            fs::read_to_string(&onespace_state).unwrap(),
            "{\"unchanged\":true}"
        );
        assert_eq!(failing.calls.load(Ordering::SeqCst), 1);

        let succeeding =
            EnvironmentUseRunner::succeeding(paths.marker.clone(), paths.agents[0].clone());
        let status = environment_use_from_paths(&paths, &repository, "next", &succeeding)
            .await
            .unwrap();
        assert_eq!(status.current, "next");
        assert!(status.exists && status.valid);
        let commands = succeeding.commands.lock().unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].args, [INSTALL_SCRIPT, "env", "use", "next"]);
        assert_eq!(commands[0].cwd.as_deref(), Some(repository.as_path()));
        drop(commands);

        fs::write(
            paths.environments.join("invalid.json"),
            r#"{"version":1,"roles":{"unknown-role":{}}}"#,
        )
        .unwrap();
        let never_called =
            EnvironmentUseRunner::succeeding(paths.marker.clone(), paths.agents[0].clone());
        assert_eq!(
            environment_use_from_paths(&paths, &repository, "invalid", &never_called)
                .await
                .unwrap_err()
                .code,
            "invalid_environment_config"
        );
        assert_eq!(never_called.calls.load(Ordering::SeqCst), 0);
        assert_eq!(fs::read_to_string(&paths.marker).unwrap(), "next");
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn managed_paths_reject_symlinks_and_non_directories() {
        let root = temporary_root();
        let app = root.join("app");
        fs::create_dir(&app).unwrap();
        fs::write(app.join(MANAGED_DIRECTORY), "not a directory").unwrap();
        assert_eq!(
            managed_paths_from_app_dir(app.clone()).unwrap_err().code,
            "unsafe_path"
        );
        fs::remove_file(app.join(MANAGED_DIRECTORY)).unwrap();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, app.join(MANAGED_DIRECTORY)).unwrap();
            assert_eq!(
                managed_paths_from_app_dir(app).unwrap_err().code,
                "unsafe_path"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repository_validation_rejects_symlink_and_unexpected_files() {
        let root = temporary_root();
        let repository = root.join("repository");
        create_repository(&repository, "1.0.0");
        fs::remove_file(repository.join("package.json")).unwrap();
        fs::create_dir(repository.join("package.json")).unwrap();
        assert_eq!(
            validate_repository(&repository).unwrap_err().code,
            "unsafe_path"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn installed_true_version_none_when_package_version_is_missing_or_unparseable() {
        let app = temporary_root();
        let paths = managed_paths_from_app_dir(app.clone()).unwrap();
        create_repository(&paths.repository, "1.0.0");

        for package in ["{}", "{not json", r#"{"version":42}"#] {
            fs::write(paths.repository.join("package.json"), package).unwrap();
            assert_eq!(
                install_version_from_paths(&paths),
                InstallVersion {
                    installed: true,
                    version: None,
                    error: None,
                }
            );
        }

        fs::remove_dir_all(app).unwrap();
    }

    #[test]
    fn runtime_state_covers_lock_cancel_failure_and_success_states() {
        let mut state = RuntimeState::default();
        assert_eq!(state.status.state, InstallState::Idle);
        let cancellation = state.begin(InstallOperation::Install).unwrap();
        assert_eq!(state.status.state, InstallState::Running);
        assert!(state.begin(InstallOperation::Update).is_none());
        cancellation.cancel();
        state.finish(Err(BackendError::cancelled()));
        assert_eq!(state.status.state, InstallState::Cancelled);
        state.begin(InstallOperation::Update).unwrap();
        state.finish(Err(BackendError::new("command_failed", "failed")));
        assert_eq!(state.status.state, InstallState::Failed);
        state.begin(InstallOperation::Update).unwrap();
        state.finish(Ok(Some("2.0.0".to_string())));
        assert_eq!(state.status.state, InstallState::Succeeded);
        assert_eq!(state.status.version.as_deref(), Some("2.0.0"));
    }

    #[tokio::test]
    async fn first_install_uses_temporary_clone_then_fixed_install_order() {
        let app = temporary_root();
        let paths = managed_paths_from_app_dir(app.clone()).unwrap();
        let runner = Arc::new(FakeRunner::default());
        let result = execute_workflow(
            paths.clone(),
            InstallOperation::Install,
            runner.clone(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap().as_deref(), Some("1.2.3"));
        assert!(paths.repository.is_dir());
        let commands = runner.commands.lock().unwrap();
        let stages: Vec<_> = commands.iter().map(|command| command.stage).collect();
        assert_eq!(
            stages,
            [
                InstallStage::Clone,
                InstallStage::VerifyRepository,
                InstallStage::NpmCi,
                InstallStage::Install,
                InstallStage::Validate,
            ]
        );
        assert!(commands[0].args[2].contains("repository.install-"));
        fs::remove_dir_all(app).unwrap();
    }

    #[tokio::test]
    async fn first_install_failure_at_every_stage_never_publishes_repository() {
        for failure_index in 0..5 {
            let app = temporary_root();
            let paths = managed_paths_from_app_dir(app.clone()).unwrap();
            let results = (0..=failure_index)
                .map(|index| {
                    if index == failure_index {
                        Err(BackendError::new(
                            "command_failed",
                            format!("stage {failure_index} failed"),
                        ))
                    } else {
                        Ok(ProcessOutput {
                            stdout: if index == 1 {
                                "true\n".to_string()
                            } else {
                                String::new()
                            },
                            stderr: String::new(),
                        })
                    }
                })
                .collect();
            let result = execute_workflow(
                paths.clone(),
                InstallOperation::Install,
                Arc::new(FakeRunner::with_results(results)),
                CancellationToken::new(),
            )
            .await;
            assert!(result.is_err(), "failure index {failure_index} must fail");
            assert!(
                !paths.repository.exists(),
                "failure index {failure_index} published a repository"
            );
            assert!(fs::read_dir(&paths.root).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("repository.install-")));
            fs::remove_dir_all(app).unwrap();
        }
    }

    #[tokio::test]
    async fn cancellation_after_work_started_cleans_first_install_without_publishing() {
        let app = temporary_root();
        let paths = managed_paths_from_app_dir(app.clone()).unwrap();
        let result = execute_workflow(
            paths.clone(),
            InstallOperation::Install,
            Arc::new(FakeRunner::cancelling_after(InstallStage::NpmCi)),
            CancellationToken::new(),
        )
        .await;
        assert!(result.unwrap_err().cancelled);
        assert!(!paths.repository.exists());
        assert!(fs::read_dir(&paths.root).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("repository.install-")));
        fs::remove_dir_all(app).unwrap();
    }

    #[tokio::test]
    async fn update_constrains_origin_pulls_then_runs_fixed_install_order() {
        let app = temporary_root();
        let paths = managed_paths_from_app_dir(app.clone()).unwrap();
        create_repository(&paths.repository, "2.0.0");
        let runner = Arc::new(FakeRunner::default());
        let result = execute_workflow(
            paths,
            InstallOperation::Update,
            runner.clone(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap().as_deref(), Some("2.0.0"));
        let commands = runner.commands.lock().unwrap();
        let stages: Vec<_> = commands.iter().map(|command| command.stage).collect();
        assert_eq!(
            stages,
            [
                InstallStage::VerifyRepository,
                InstallStage::Pull,
                InstallStage::Pull,
                InstallStage::NpmCi,
                InstallStage::Install,
                InstallStage::Validate,
            ]
        );
        assert_eq!(
            commands[1].args,
            ["remote", "set-url", "origin", REPOSITORY_URL]
        );
        fs::remove_dir_all(app).unwrap();
    }

    #[tokio::test]
    async fn successful_update_does_not_fail_when_version_is_unknown() {
        let app = temporary_root();
        let paths = managed_paths_from_app_dir(app.clone()).unwrap();
        create_repository(&paths.repository, "2.0.0");
        fs::write(paths.repository.join("package.json"), "{}").unwrap();

        let result = execute_workflow(
            paths,
            InstallOperation::Update,
            Arc::new(FakeRunner::default()),
            CancellationToken::new(),
        )
        .await;

        assert_eq!(result.unwrap(), None);
        fs::remove_dir_all(app).unwrap();
    }

    #[tokio::test]
    async fn update_failure_keeps_the_formal_repository_byte_identical() {
        let app = temporary_root();
        let paths = managed_paths_from_app_dir(app.clone()).unwrap();
        create_repository(&paths.repository, "2.0.0");
        let original = fs::read(paths.repository.join("package.json")).unwrap();
        let runner = Arc::new(FakeRunner::with_results(vec![
            Ok(ProcessOutput {
                stdout: "true\n".to_string(),
                stderr: String::new(),
            }),
            Ok(ProcessOutput {
                stdout: String::new(),
                stderr: String::new(),
            }),
            Err(BackendError::new("command_failed", "pull failed")),
        ]));
        let result = execute_workflow(
            paths.clone(),
            InstallOperation::Update,
            runner,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap_err().code, "command_failed");
        assert_eq!(
            fs::read(paths.repository.join("package.json")).unwrap(),
            original
        );
        assert!(paths.repository.is_dir());
        assert!(fs::read_dir(&paths.root).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.starts_with("repository.install-") && !name.starts_with("repository.backup-")
        }));
        fs::remove_dir_all(app).unwrap();
    }

    #[tokio::test]
    async fn failure_stops_later_stages_and_preserves_ordered_logs() {
        let app = temporary_root();
        let paths = managed_paths_from_app_dir(app.clone()).unwrap();
        create_repository(&paths.repository, "2.0.0");
        let failure =
            BackendError::new("command_failed", "pull failed").with_output(ProcessOutput {
                stdout: String::new(),
                stderr: "network unavailable".to_string(),
            });
        let runner = Arc::new(FakeRunner::with_results(vec![
            Ok(ProcessOutput {
                stdout: "true\n".to_string(),
                stderr: String::new(),
            }),
            Ok(ProcessOutput {
                stdout: String::new(),
                stderr: String::new(),
            }),
            Err(failure),
        ]));
        let result = execute_workflow(
            paths,
            InstallOperation::Update,
            runner.clone(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap_err().code, "command_failed");
        assert_eq!(runner.calls.load(Ordering::SeqCst), 3);
        let logs = locked_runtime().logs.clone();
        assert!(logs
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence));
        assert!(logs
            .iter()
            .any(|entry| entry.message.contains("pull failed")));
        assert!(logs.iter().any(|entry| {
            entry.source == LogSource::Stderr && entry.message == "network unavailable"
        }));
        fs::remove_dir_all(app).unwrap();
    }

    #[tokio::test]
    async fn cancellation_stops_before_starting_later_stages() {
        let app = temporary_root();
        let paths = managed_paths_from_app_dir(app.clone()).unwrap();
        create_repository(&paths.repository, "2.0.0");
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let runner = Arc::new(FakeRunner::default());
        let result = execute_workflow(
            paths,
            InstallOperation::Update,
            runner.clone(),
            cancellation,
        )
        .await;
        assert!(result.unwrap_err().cancelled);
        assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
        fs::remove_dir_all(app).unwrap();
    }

    #[tokio::test]
    async fn registered_environment_commands_drive_real_use_and_preserve_onespace_state() {
        let home = TemporaryHome::new();
        let valid = valid_environment_content().to_string();
        let onespace_state = home.path.join(".config/onespace/providers.json");
        fs::create_dir_all(onespace_state.parent().unwrap()).unwrap();
        fs::write(&onespace_state, b"{\"unchanged\":true}").unwrap();
        let repository = home.path.join(".config/onespace/ai-work-flow/repository");
        create_repository(&repository, "1.0.0");
        fs::write(
            repository.join(INSTALL_SCRIPT),
            r#"import { mkdirSync, writeFileSync } from "node:fs";

const home = process.env.HOME;
const name = process.argv[4];
const configRoot = `${home}/.config/ai-work-flow`;
const agents = `${home}/.claude/agents`;
if (name === "broken") {
  writeFileSync(`${configRoot}/.environment`, "partial");
  mkdirSync(agents, { recursive: true });
  writeFileSync(`${agents}/managed.md`, "partial-agents");
  process.stderr.write("simulated environment failure\n");
  process.exit(17);
}
writeFileSync(`${configRoot}/.environment`, name);
mkdirSync(agents, { recursive: true });
writeFileSync(`${agents}/managed.md`, "new-agents");
"#,
        )
        .unwrap();
        assert!(ai_work_flow_environment_create("next".to_string(), valid.clone()).is_ok());
        assert!(ai_work_flow_environment_create("broken".to_string(), valid).is_ok());
        let list = ai_work_flow_environment_list().unwrap();
        assert!(list.iter().all(|entry| entry.valid));
        assert_eq!(
            ai_work_flow_environment_read("next".to_string())
                .unwrap()
                .content,
            valid_environment_content()
        );
        assert_eq!(
            ai_work_flow_environment_status().unwrap().current,
            DEFAULT_ENVIRONMENT
        );

        let marker = home.path.join(".config/ai-work-flow/.environment");
        let agents = home.path.join(".claude/agents");
        fs::write(&marker, "next").unwrap();
        fs::create_dir_all(&agents).unwrap();
        fs::write(agents.join("managed.md"), "old-agents").unwrap();
        let failed = ai_work_flow_environment_use("broken".to_string())
            .await
            .unwrap_err();
        assert_eq!(failed.code, "command_failed");
        assert_eq!(fs::read_to_string(&marker).unwrap(), "next");
        assert_eq!(
            fs::read_to_string(agents.join("managed.md")).unwrap(),
            "old-agents"
        );

        let used = ai_work_flow_environment_use("next".to_string())
            .await
            .unwrap();
        assert_eq!(used.current, "next");
        assert_eq!(
            fs::read_to_string(agents.join("managed.md")).unwrap(),
            "new-agents"
        );
        assert_eq!(ai_work_flow_environment_status().unwrap().current, "next");

        assert_eq!(
            ai_work_flow_environment_create(
                "../escape".to_string(),
                valid_environment_content().to_string()
            )
            .unwrap_err()
            .code,
            "invalid_environment_name"
        );
        assert!(!home.path.join(".config/escape.json").exists());
        assert_eq!(
            ai_work_flow_environment_delete("next".to_string())
                .unwrap()
                .current,
            DEFAULT_ENVIRONMENT
        );
        assert!(!marker.exists());
        assert_eq!(fs::read(&onespace_state).unwrap(), b"{\"unchanged\":true}");
    }

    #[cfg(unix)]
    fn process_is_alive(pid: u32) -> bool {
        unsafe { kill(pid as i32, 0) == 0 }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn system_runner_cancels_real_process_group_and_allows_next_command() {
        let app = temporary_root();
        let script = app.join("long-running.mjs");
        fs::write(
            &script,
            r#"import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";

const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], {
  stdio: ["ignore", "ignore", "ignore"]
});
writeFileSync("descendant.pid", String(child.pid));
process.stdout.write("long-running\n");
setInterval(() => {}, 1000);
"#,
        )
        .unwrap();
        let command = CommandSpec {
            executable: Executable::Node,
            args: vec![script.file_name().unwrap().to_string_lossy().into_owned()],
            cwd: Some(app.clone()),
            stage: InstallStage::Install,
        };
        let cancellation = CancellationToken::new();
        let runner = Arc::new(SystemProcessRunner);
        let task = {
            let runner = runner.clone();
            let cancellation = cancellation.clone();
            tokio::spawn(async move { run_command(runner.as_ref(), &command, &cancellation).await })
        };
        let pid_path = app.join("descendant.pid");
        for _ in 0..100 {
            if pid_path.is_file() {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert!(
            pid_path.is_file(),
            "long-running command did not spawn child"
        );
        let descendant_pid: u32 = fs::read_to_string(&pid_path).unwrap().parse().unwrap();
        cancellation.cancel();
        let error = task.await.unwrap().unwrap_err();
        assert!(error.cancelled);
        assert!(error
            .output
            .as_ref()
            .is_some_and(|output| output.stdout.contains("long-running")));
        for _ in 0..100 {
            if !process_is_alive(descendant_pid) {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert!(!process_is_alive(descendant_pid));
        assert!(locked_runtime()
            .logs
            .iter()
            .any(|entry| entry.message.contains("long-running")));

        let next = CommandSpec {
            executable: Executable::Node,
            args: vec!["-e".to_string(), "process.stdout.write('next')".to_string()],
            cwd: Some(app.clone()),
            stage: InstallStage::Validate,
        };
        let output = run_command(runner.as_ref(), &next, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(output.stdout, "next");
        fs::remove_dir_all(app).unwrap();
    }
}
