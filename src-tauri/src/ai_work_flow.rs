use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const REPOSITORY_URL: &str = "https://github.com/hengboy/ai-work-flow.git";
const MANAGED_DIRECTORY: &str = "ai-work-flow";
const REPOSITORY_DIRECTORY: &str = "repository";
const INSTALL_SCRIPT: &str = "agent-build/install.mjs";
const ENVIRONMENTS_DIRECTORY: &str = "environments";
const ENVIRONMENT_MARKER: &str = ".environment";
const DEFAULT_ENVIRONMENT: &str = "default";

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

    fn finish(&mut self, result: Result<String, BackendError>) {
        self.status.finished_at = Some(now());
        self.cancellation = None;
        match result {
            Ok(version) => {
                self.status.state = InstallState::Succeeded;
                self.status.stage = Some(InstallStage::Complete);
                self.status.version = Some(version);
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
    root: PathBuf,
    environments: PathBuf,
    marker: PathBuf,
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
        marker: root.join(ENVIRONMENT_MARKER),
        root,
        environments,
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

fn parse_environment_content(content: &str) -> Result<Value, BackendError> {
    let value: Value = serde_json::from_str(content).map_err(|error| {
        BackendError::new(
            "invalid_environment_json",
            format!("Environment is not valid JSON: {error}"),
        )
    })?;
    if !value.is_object() {
        return Err(BackendError::new(
            "invalid_environment_config",
            "Environment configuration must be a JSON object",
        ));
    }
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
    if current == DEFAULT_ENVIRONMENT && !paths.marker.exists() {
        return Ok(EnvironmentStatus {
            current,
            exists: true,
            valid: true,
        });
    }
    let path = environment_file(paths, &current)?;
    let exists = inspect_optional_file(&path, "Current environment file")?;
    let valid = exists
        && fs::read_to_string(path)
            .ok()
            .and_then(|content| parse_environment_content(&content).ok())
            .is_some();
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

fn read_version(path: &Path) -> Result<String, BackendError> {
    validate_repository(path)?;
    let package_path = path.join("package.json");
    let content = fs::read_to_string(&package_path)
        .map_err(|error| io_error("Cannot read managed repository package.json", error))?;
    let package: serde_json::Value = serde_json::from_str(&content).map_err(|error| {
        BackendError::new("version_invalid", format!("Invalid package.json: {error}"))
    })?;
    let version = package
        .get("version")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| BackendError::new("version_invalid", "package.json has no version"))?;
    Ok(version.to_string())
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
            if let Some(cwd) = command.cwd.as_deref() {
                process.current_dir(cwd);
            }
            let mut child = process.spawn().map_err(|error| {
                BackendError::new(
                    "spawn_failed",
                    format!("Cannot start {}: {error}", command.executable.name()),
                )
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
                    let _ = child.kill().await;
                    let status = child.wait().await
                        .map_err(|error| io_error("Cannot reap cancelled child process", error))?;
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

async fn install_repository(
    paths: &ManagedPaths,
    runner: &dyn ProcessRunner,
    cancellation: &CancellationToken,
) -> Result<(), BackendError> {
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
        fs::rename(&temporary, &paths.repository)
            .map_err(|error| io_error("Cannot publish managed repository", error))?;
        Ok(())
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

async fn update_repository(
    paths: &ManagedPaths,
    runner: &dyn ProcessRunner,
    cancellation: &CancellationToken,
) -> Result<(), BackendError> {
    validate_repository(&paths.repository)?;
    let verify = run_command(
        runner,
        &CommandSpec::verify(&paths.repository),
        cancellation,
    )
    .await?;
    if verify.stdout.trim() != "true" {
        return Err(BackendError::new(
            "repository_invalid",
            "Managed repository is not a Git work tree",
        ));
    }
    run_command(
        runner,
        &CommandSpec::constrain_origin(&paths.repository),
        cancellation,
    )
    .await?;
    run_command(runner, &CommandSpec::pull(&paths.repository), cancellation).await?;
    validate_repository(&paths.repository)
}

async fn execute_workflow(
    paths: ManagedPaths,
    operation: InstallOperation,
    runner: Arc<dyn ProcessRunner>,
    cancellation: CancellationToken,
) -> Result<String, BackendError> {
    locked_runtime().log(
        InstallStage::Preparing,
        LogSource::System,
        format!("Starting {operation:?}"),
    );
    match operation {
        InstallOperation::Install => {
            install_repository(&paths, runner.as_ref(), &cancellation).await?
        }
        InstallOperation::Update => {
            update_repository(&paths, runner.as_ref(), &cancellation).await?
        }
    }
    validate_repository(&paths.repository)?;
    for command in [
        CommandSpec::npm_ci(&paths.repository),
        CommandSpec::install(&paths.repository),
        CommandSpec::validate(&paths.repository),
    ] {
        run_command(runner.as_ref(), &command, &cancellation).await?;
    }
    let version = read_version(&paths.repository)?;
    if cancellation.is_cancelled() {
        return Err(BackendError::cancelled());
    }
    Ok(version)
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
    let snapshot = marker_snapshot(paths)?;
    let result = runner.run(&command, CancellationToken::new()).await;
    if let Err(error) = result {
        restore_marker(paths, snapshot.as_deref())?;
        return Err(error);
    }
    match read_current_environment(paths) {
        Ok(current) if current == name => environment_status_from_paths(paths),
        _ => {
            restore_marker(paths, snapshot.as_deref())?;
            Err(BackendError::new(
                "environment_switch_incomplete",
                "AI Work Flow did not activate the requested environment",
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
    let result = managed_paths().and_then(|paths| match repository_operation(&paths.repository)? {
        InstallOperation::Install => Ok(None),
        InstallOperation::Update => read_version(&paths.repository).map(Some),
    });
    match result {
        Ok(version) => InstallVersion {
            installed: version.is_some(),
            version,
            error: None,
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
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    #[derive(Default)]
    struct FakeRunner {
        commands: Mutex<Vec<CommandSpec>>,
        results: Mutex<VecDeque<Result<ProcessOutput, BackendError>>>,
        calls: AtomicUsize,
    }

    impl FakeRunner {
        fn with_results(results: Vec<Result<ProcessOutput, BackendError>>) -> Self {
            Self {
                results: Mutex::new(results.into()),
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
                self.results.lock().unwrap().pop_front().unwrap_or_else(|| {
                    Ok(ProcessOutput {
                        stdout: if command.stage == InstallStage::VerifyRepository {
                            "true\n".to_string()
                        } else {
                            String::new()
                        },
                        stderr: String::new(),
                    })
                })
            })
        }
    }

    struct EnvironmentUseRunner {
        marker: PathBuf,
        fail: bool,
        mutate_marker_before_failure: bool,
        commands: Mutex<Vec<CommandSpec>>,
        calls: AtomicUsize,
    }

    impl EnvironmentUseRunner {
        fn succeeding(marker: PathBuf) -> Self {
            Self {
                marker,
                fail: false,
                mutate_marker_before_failure: false,
                commands: Mutex::new(Vec::new()),
                calls: AtomicUsize::new(0),
            }
        }

        fn failing(marker: PathBuf, mutate_marker_before_failure: bool) -> Self {
            Self {
                marker,
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
        let original = "{\n  \"agents\": {\"claude\": true},\n  \"unknown\": [1, 2]\n}\n";
        let created = environment_create_from_paths(&paths, "team", original).unwrap();
        assert_eq!(created.content, original);
        assert_eq!(created.value.unwrap()["unknown"], serde_json::json!([1, 2]));

        let updated = "{\"agents\":{},\"extra\":{\"preserved\":true}}";
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

        environment_create_from_paths(&paths, "stable", "{\"value\":1}").unwrap();
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
        environment_create_from_paths(&paths, "current", "{}").unwrap();
        environment_create_from_paths(&paths, "other", "{}").unwrap();
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
        environment_create_from_paths(&paths, "old", "{}").unwrap();
        environment_create_from_paths(&paths, "next", "{\"agents\":{}}").unwrap();
        fs::write(&paths.marker, "old").unwrap();
        let repository = temporary.join("repository");
        create_repository(&repository, "1.0.0");
        let onespace_state = temporary.join("onespace-ai-environments.json");
        fs::write(&onespace_state, "{\"unchanged\":true}").unwrap();

        let failing = EnvironmentUseRunner::failing(paths.marker.clone(), true);
        assert_eq!(
            environment_use_from_paths(&paths, &repository, "next", &failing)
                .await
                .unwrap_err()
                .code,
            "command_failed"
        );
        assert_eq!(fs::read_to_string(&paths.marker).unwrap(), "old");
        assert_eq!(
            fs::read_to_string(&onespace_state).unwrap(),
            "{\"unchanged\":true}"
        );
        assert_eq!(failing.calls.load(Ordering::SeqCst), 1);

        let succeeding = EnvironmentUseRunner::succeeding(paths.marker.clone());
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

        fs::write(paths.environments.join("invalid.json"), "[]").unwrap();
        let never_called = EnvironmentUseRunner::succeeding(paths.marker.clone());
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
        state.finish(Ok("2.0.0".to_string()));
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
        assert_eq!(result.unwrap(), "1.2.3");
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
        assert_eq!(result.unwrap(), "2.0.0");
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
}
