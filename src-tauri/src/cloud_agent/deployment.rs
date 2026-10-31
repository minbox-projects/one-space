use std::collections::HashSet;
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader as TokioBufReader};
use tokio::process::{Child as TokioChild, Command as TokioCommand};
use tokio::time::timeout as tokio_timeout;

use crate::cloud_agent::types::{
    CloudAgentDeployPhase, CloudAgentDeployPhaseStatus, CloudAgentDeploymentProgress,
    CloudAgentDeploymentResult, CloudAgentInstallationInfo, CloudAgentInstallationState,
};

const REQUIRED_PAYLOAD_FILES: [&str; 8] = [
    "agent.py",
    "core/__init__.py",
    "core/disk_guardian.py",
    "core/gateway.py",
    "core/issue_worker.py",
    "core/notifier.py",
    "core/session_runner.py",
    "config.json",
];

const DEPLOYABLE_FILES: [&str; 13] = [
    "deploy.sh",
    "bootstrap.sh",
    "VERSION",
    "config.default.json",
    "config.example.json",
    "agent.py",
    "core/__init__.py",
    "core/disk_guardian.py",
    "core/gateway.py",
    "core/issue_worker.py",
    "core/notifier.py",
    "core/session_runner.py",
    "systemd/onespace-cloud-agent.service",
];

const SSH_READ_TIMEOUT: Duration = Duration::from_secs(12);

pub trait RemoteShell: Send + Sync {
    fn run(&self, host: &str, command: &str) -> Result<String, String>;
}

pub struct SshRemoteShell;

impl RemoteShell for SshRemoteShell {
    fn run(&self, host: &str, command: &str) -> Result<String, String> {
        run_ssh_command_blocking(host, command, SSH_READ_TIMEOUT)
    }
}

impl CloudAgentDeployPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Platform => "platform",
            Self::Dependencies => "dependencies",
            Self::Upload => "upload",
            Self::Config => "config",
            Self::Publish => "publish",
            Self::Service => "service",
            Self::Metadata => "metadata",
            Self::Cleanup => "cleanup",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "platform" => Some(Self::Platform),
            "dependencies" => Some(Self::Dependencies),
            "upload" => Some(Self::Upload),
            "config" => Some(Self::Config),
            "publish" => Some(Self::Publish),
            "service" => Some(Self::Service),
            "metadata" => Some(Self::Metadata),
            "cleanup" => Some(Self::Cleanup),
            _ => None,
        }
    }
}

impl CloudAgentDeployPhaseStatus {
    fn from_key(key: &str) -> Option<Self> {
        match key {
            "start" => Some(Self::Start),
            "ok" => Some(Self::Ok),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

pub fn classify_payload(entries: &[String]) -> CloudAgentInstallationState {
    let present: HashSet<&str> = entries
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .collect();
    let matched = REQUIRED_PAYLOAD_FILES
        .iter()
        .filter(|required| present.contains(**required))
        .count();

    if matched == 0 {
        CloudAgentInstallationState::NotInstalled
    } else if matched == REQUIRED_PAYLOAD_FILES.len() {
        CloudAgentInstallationState::Installed
    } else {
        CloudAgentInstallationState::Incomplete
    }
}

pub fn parse_install_metadata(
    remote_version: Option<&str>,
    metadata: Option<&str>,
) -> (Option<String>, Option<String>, Option<String>) {
    let version = remote_version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let Some(raw) = metadata.map(str::trim).filter(|value| !value.is_empty()) else {
        return (version, None, None);
    };

    let parsed: serde_json::Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(error) => {
            return (
                version,
                None,
                Some(format!("installation metadata is not valid JSON: {}", error)),
            )
        }
    };

    let Some(metadata_version) = parsed
        .get("version")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (
            version,
            None,
            Some("installation metadata is missing a version".to_string()),
        );
    };

    match version.as_deref() {
        Some(current) if current == metadata_version => {}
        Some(_) => {
            return (
                version,
                None,
                Some("installation metadata version does not match the remote VERSION".to_string()),
            )
        }
        None => {
            return (
                version,
                None,
                Some(
                    "installation metadata cannot be verified without a remote VERSION".to_string(),
                ),
            )
        }
    }

    let Some(timestamp) = parsed
        .get("installed_at")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (
            version,
            None,
            Some("installation metadata is missing installed_at".to_string()),
        );
    };

    match chrono::DateTime::parse_from_rfc3339(timestamp) {
        Ok(parsed_time) if parsed_time.timestamp() > 0 => {
            (version, Some(timestamp.to_string()), None)
        }
        Ok(_) => (
            version,
            None,
            Some("installation metadata timestamp is not a valid installation time".to_string()),
        ),
        Err(error) => (
            version,
            None,
            Some(format!(
                "installation metadata timestamp is malformed: {}",
                error
            )),
        ),
    }
}

pub fn parse_phase_line(
    line: &str,
) -> Option<(
    CloudAgentDeployPhase,
    CloudAgentDeployPhaseStatus,
    Option<String>,
)> {
    let rest = line.trim().strip_prefix("ONESPACE_DEPLOY_PHASE ")?;
    let mut phase = None;
    let mut status = None;
    let mut reason = None;

    for token in rest.split_whitespace() {
        if let Some(value) = token.strip_prefix("phase=") {
            phase = CloudAgentDeployPhase::from_key(value);
            phase?;
        } else if let Some(value) = token.strip_prefix("status=") {
            status = CloudAgentDeployPhaseStatus::from_key(value);
            status?;
        } else if let Some(value) = token.strip_prefix("reason=") {
            reason = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        } else {
            return None;
        }
    }

    Some((phase?, status?, reason))
}

pub fn inspect_installation(
    host: &str,
    agent_dir: &str,
    local_latest_version: &str,
    shell: &dyn RemoteShell,
) -> CloudAgentInstallationInfo {
    let mut info = CloudAgentInstallationInfo {
        host: host.to_string(),
        state: CloudAgentInstallationState::NotInstalled,
        local_latest_version: local_latest_version.to_string(),
        remote_version: None,
        installed_at: None,
        metadata_reason: None,
        configuration_error: None,
        service_active: None,
        probe_reason: None,
    };

    let raw = match shell.run(host, &build_presence_command(agent_dir)) {
        Ok(output) => output,
        Err(error) => {
            info.state = CloudAgentInstallationState::ProbeError;
            info.probe_reason = Some(error);
            return info;
        }
    };

    let entries: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    info.state = classify_payload(&entries);

    if info.state == CloudAgentInstallationState::NotInstalled {
        if !entries.is_empty() {
            info.state = CloudAgentInstallationState::ProbeError;
            info.probe_reason = Some(
                "destination is nonempty but contains no recognized Cloud Agent payload; resolve the conflict manually"
                    .to_string(),
            );
        }
        return info;
    }

    if entries.iter().any(|entry| entry == "config.json") {
        let command = format!(
            "python3 -m json.tool {} >/dev/null 2>&1",
            shell_quote(&remote_join(agent_dir, "config.json"))
        );
        if let Err(error) = shell.run(host, &command) {
            info.configuration_error = Some(error);
        }
    }

    let version = read_optional(
        shell,
        host,
        &format!(
            "cat {} 2>/dev/null",
            shell_quote(&remote_join(agent_dir, "VERSION"))
        ),
    );
    let metadata = read_optional(
        shell,
        host,
        &format!(
            "cat {} 2>/dev/null",
            shell_quote(&remote_join(agent_dir, "install-info.json"))
        ),
    );
    let (remote_version, installed_at, metadata_reason) =
        parse_install_metadata(version.as_deref(), metadata.as_deref());
    info.remote_version = remote_version;
    info.installed_at = installed_at;
    info.metadata_reason = metadata_reason;

    if info.state == CloudAgentInstallationState::Installed {
        if let Ok(raw) = shell.run(host, &build_status_command(agent_dir)) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                info.service_active = value
                    .get("service_active")
                    .and_then(|active| active.as_bool());
            }
        }
    }

    info
}

pub fn validate_local_payload(payload_dir: &Path) -> Result<String, String> {
    for relative in DEPLOYABLE_FILES {
        let path = payload_dir.join(relative);
        if !path.is_file() {
            return Err(format!(
                "bundled Cloud Agent payload is missing {}",
                relative
            ));
        }
    }

    let version = std::fs::read_to_string(payload_dir.join("VERSION"))
        .map_err(|error| format!("failed to read bundled Cloud Agent VERSION: {}", error))?
        .trim()
        .to_string();
    if version.is_empty() {
        return Err("bundled Cloud Agent VERSION is empty".to_string());
    }

    Ok(version)
}

pub fn read_install_metadata(
    host: &str,
    agent_dir: &str,
    remote_version: &str,
    shell: &dyn RemoteShell,
) -> (Option<String>, Option<String>, Option<String>) {
    let metadata = read_optional(
        shell,
        host,
        &format!(
            "cat {} 2>/dev/null",
            shell_quote(&remote_join(agent_dir, "install-info.json"))
        ),
    );
    parse_install_metadata(Some(remote_version), metadata.as_deref())
}

pub fn build_status_command(agent_dir: &str) -> String {
    let dir = normalized_dir(agent_dir);
    format!(
        "python3 \"{}/agent.py\" status \"{}/config.json\" --json",
        dir, dir
    )
}

struct ActiveDeployment {
    host: String,
    path: String,
    operation_id: String,
}

#[derive(Default)]
struct RegistryState {
    active: Option<ActiveDeployment>,
    retry_proof: HashSet<(String, String)>,
}

pub struct DeploymentRegistry {
    state: Mutex<RegistryState>,
}

impl DeploymentRegistry {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(RegistryState::default()),
        }
    }

    pub fn begin(
        &self,
        host: String,
        path: String,
        operation_id: String,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "deployment registry is unavailable".to_string())?;
        if let Some(active) = &state.active {
            if active.operation_id == operation_id {
                return Err(format!(
                    "deployment operation {} is already running",
                    operation_id
                ));
            }
            return Err(format!(
                "another deployment is already running for {}",
                active.host
            ));
        }
        state.active = Some(ActiveDeployment {
            host,
            path,
            operation_id,
        });
        Ok(())
    }

    pub fn finish(&self, operation_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            let matches = state
                .active
                .as_ref()
                .map(|active| active.operation_id == operation_id)
                .unwrap_or(false);
            if matches {
                state.active = None;
            }
        }
    }

    pub fn record_failed(&self, host: String, path: String) {
        if let Ok(mut state) = self.state.lock() {
            state.retry_proof.insert((host, path));
        }
    }

    pub fn may_deploy(
        &self,
        host: &str,
        path: &str,
        state: CloudAgentInstallationState,
    ) -> Result<(), String> {
        match state {
            CloudAgentInstallationState::NotInstalled
            | CloudAgentInstallationState::Installed => Ok(()),
            CloudAgentInstallationState::Incomplete => {
                let registry = self
                    .state
                    .lock()
                    .map_err(|_| "deployment registry is unavailable".to_string())?;
                if registry
                    .retry_proof
                    .contains(&(host.to_string(), path.to_string()))
                {
                    Ok(())
                } else {
                    Err(
                        "a partial installation is present without retry proof from this session; use the manual recovery procedure"
                            .to_string(),
                    )
                }
            }
            CloudAgentInstallationState::ProbeError => Err(
                "the remote installation probe failed; resolve the target state manually before deploying"
                    .to_string(),
            ),
        }
    }
}

impl Default for DeploymentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Owns the deployer process and its process group for the lifetime of a
/// deployment attempt. Dropping the guard (including when the awaiting command
/// future is cancelled because the application is exiting) kills the whole
/// local process group and reaps the direct child.
struct DeployProcessGuard {
    child: TokioChild,
    reaped: bool,
    killed: bool,
}

impl DeployProcessGuard {
    fn new(child: TokioChild) -> Self {
        Self {
            child,
            reaped: false,
            killed: false,
        }
    }

    fn kill_group(&mut self) {
        if self.killed {
            return;
        }
        self.killed = true;
        #[cfg(unix)]
        {
            if let Some(pid) = self.child.id() {
                let _ = Command::new("kill")
                    .arg("-KILL")
                    .arg(format!("-{}", pid))
                    .status();
            }
        }
        let _ = self.child.start_kill();
    }

    fn disarm(&mut self) {
        self.reaped = true;
    }
}

impl Drop for DeployProcessGuard {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        self.kill_group();
        // Reap the direct child without awaiting: the runtime or its process
        // driver may already be shutting down when this future is dropped.
        for _ in 0..200 {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => thread::sleep(Duration::from_millis(5)),
                Err(_) => return,
            }
        }
    }
}

pub fn run_deploy_process(
    deployer: &Path,
    local_version: &str,
    host: &str,
    agent_dir: &str,
    operation_id: &str,
    timeout: Duration,
    on_progress: impl FnMut(CloudAgentDeploymentProgress),
) -> Result<CloudAgentDeploymentResult, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start deployment runtime: {}", error))?;
    runtime.block_on(run_deploy_process_async(
        deployer,
        local_version,
        host,
        agent_dir,
        operation_id,
        timeout,
        on_progress,
    ))
}

/// Cancellation-safe deployer runner. The returned future may be dropped at any
/// await point; its drop guard kills the local deployer process group. A timeout
/// or application exit only stops local processes and cannot roll back remote
/// package installation or publication.
pub async fn run_deploy_process_async(
    deployer: &Path,
    local_version: &str,
    host: &str,
    agent_dir: &str,
    operation_id: &str,
    timeout: Duration,
    mut on_progress: impl FnMut(CloudAgentDeploymentProgress),
) -> Result<CloudAgentDeploymentResult, String> {
    let mut command = TokioCommand::new("bash");
    command
        .arg(deployer)
        .arg(host)
        .arg(agent_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    configure_tokio_process_group(&mut command);

    let child = command
        .spawn()
        .map_err(|error| format!("failed to start Cloud Agent deployer: {}", error))?;
    let mut guard = DeployProcessGuard::new(child);
    let stdout = guard
        .child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture deployer output".to_string())?;
    let stderr = guard
        .child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture deployer errors".to_string())?;

    let mut stdout_lines = TokioBufReader::new(stdout).lines();
    let stderr_task = tokio::spawn(async move {
        let mut buffer = String::new();
        let mut reader = TokioBufReader::new(stderr);
        let _ = reader.read_to_string(&mut buffer).await;
        buffer
    });

    let deadline = Instant::now() + timeout;
    let mut last_phase = None;
    let mut timed_out = false;

    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            timed_out = true;
            break;
        };
        match tokio_timeout(remaining, stdout_lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                if let Some((phase, status, reason)) = parse_phase_line(&line) {
                    on_progress(CloudAgentDeploymentProgress {
                        host: host.to_string(),
                        operation_id: operation_id.to_string(),
                        phase,
                        status,
                        reason: reason.clone(),
                    });
                    last_phase = Some((phase, status, reason));
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(_)) => break,
            Err(_) => {
                timed_out = true;
                break;
            }
        }
    }

    if timed_out {
        guard.kill_group();
        let _ = stderr_task.await;
        return Err(format!(
            "deployment timeout after {} seconds",
            timeout.as_secs()
        ));
    }

    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
        guard.kill_group();
        let _ = stderr_task.await;
        return Err(format!(
            "deployment timeout after {} seconds",
            timeout.as_secs()
        ));
    };

    let exit_status = match tokio_timeout(remaining, guard.child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            guard.kill_group();
            let _ = stderr_task.await;
            return Err(format!("failed to wait for deployment process: {}", error));
        }
        Err(_) => {
            guard.kill_group();
            let _ = stderr_task.await;
            return Err(format!(
                "deployment timeout after {} seconds",
                timeout.as_secs()
            ));
        }
    };
    guard.disarm();

    let stderr_text = stderr_task
        .await
        .unwrap_or_default()
        .trim()
        .to_string();

    if !exit_status.success() {
        let (phase_label, reason) = match &last_phase {
            Some((phase, _, reason)) => (phase.as_str().to_string(), reason.clone()),
            None => ("deploy".to_string(), None),
        };
        let detail = reason
            .filter(|value| !value.is_empty())
            .or_else(|| {
                if stderr_text.is_empty() {
                    None
                } else {
                    Some(stderr_text)
                }
            })
            .unwrap_or_else(|| format!("exit code {:?}", exit_status.code()));
        return Err(format!(
            "deployment failed in phase {}: {}",
            phase_label, detail
        ));
    }

    let phase = last_phase
        .map(|(phase, _, _)| phase)
        .unwrap_or(CloudAgentDeployPhase::Cleanup);
    Ok(CloudAgentDeploymentResult {
        host: host.to_string(),
        operation_id: operation_id.to_string(),
        success: true,
        phase,
        reason: None,
        remote_version: Some(local_version.to_string()),
        installed_at: None,
    })
}

fn build_presence_command(agent_dir: &str) -> String {
    let dir = normalized_dir(agent_dir);
    let mut command = format!("if [ -d {} ]; then found=0; ", shell_quote(dir));
    for relative in REQUIRED_PAYLOAD_FILES {
        command.push_str(&format!(
            "if [ -e {} ]; then printf '%s\\n' {}; found=1; fi; ",
            shell_quote(&remote_join(dir, relative)),
            shell_quote(relative)
        ));
    }
    command.push_str(&format!(
        "if [ \"$found\" -eq 0 ]; then ls -A {} 2>/dev/null || true; fi; fi",
        shell_quote(dir)
    ));
    command
}

fn read_optional(shell: &dyn RemoteShell, host: &str, command: &str) -> Option<String> {
    shell
        .run(host, command)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalized_dir(agent_dir: &str) -> &str {
    let trimmed = agent_dir.trim_end_matches('/');
    if trimmed.is_empty() {
        "/"
    } else {
        trimmed
    }
}

fn remote_join(agent_dir: &str, relative: &str) -> String {
    let base = normalized_dir(agent_dir);
    if base == "/" {
        format!("/{}", relative)
    } else {
        format!("{}/{}", base, relative)
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

fn configure_tokio_process_group(command: &mut TokioCommand) {
    #[cfg(unix)]
    {
        command.process_group(0);
    }
}

fn kill_child_group(child: &mut Child) {
    #[cfg(unix)]
    {
        let pid = child.id();
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(format!("-{}", pid))
            .status();
    }
    let _ = child.kill();
}

fn wait_for_child(
    child: &mut Child,
    deadline: Instant,
    timeout: Duration,
) -> Result<ExitStatus, String> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_child_group(child);
                    let _ = child.wait();
                    return Err(format!(
                        "deployment timeout after {} seconds",
                        timeout.as_secs()
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                kill_child_group(child);
                return Err(format!("failed to wait for deployment process: {}", error));
            }
        }
    }
}

fn run_ssh_command_blocking(
    host: &str,
    command: &str,
    timeout: Duration,
) -> Result<String, String> {
    let mut ssh = Command::new("ssh");
    ssh.arg("-o")
        .arg("ConnectTimeout=5")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg(host)
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut ssh);

    let mut child = ssh
        .spawn()
        .map_err(|error| format!("failed to start SSH command: {}", error))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture SSH output".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture SSH errors".to_string())?;

    let stdout_thread = thread::spawn(move || {
        let mut text = String::new();
        let _ = BufReader::new(stdout).read_to_string(&mut text);
        text
    });
    let stderr_thread = thread::spawn(move || {
        let mut text = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut text);
        text
    });

    let deadline = Instant::now() + timeout;
    let status = match wait_for_child(&mut child, deadline, timeout) {
        Ok(status) => status,
        Err(error) => return Err(format!("SSH command timeout: {}", error)),
    };

    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();

    if !status.success() {
        let trimmed = stderr.trim();
        return Err(if trimmed.is_empty() {
            format!("remote command failed with exit code {:?}", status.code())
        } else {
            trimmed.to_string()
        });
    }

    Ok(stdout.trim().to_string())
}
