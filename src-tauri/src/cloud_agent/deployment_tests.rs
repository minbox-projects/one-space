use crate::cloud_agent::deployment;
use crate::cloud_agent::types::{
    CloudAgentDeployPhase, CloudAgentDeployPhaseStatus, CloudAgentDeploymentProgress,
    CloudAgentDeploymentResult, CloudAgentInstallationInfo, CloudAgentInstallationState,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

struct FakeShell {
    fail: bool,
    files: String,
    version: Option<String>,
    metadata: Option<String>,
    service: String,
    config: Option<String>,
    commands: Mutex<Vec<String>>,
}

impl deployment::RemoteShell for FakeShell {
    fn run(&self, _host: &str, command: &str) -> Result<String, String> {
        if self.fail {
            return Err("permission denied".to_string());
        }
        self.commands.lock().unwrap().push(command.to_string());
        if command.contains(" status ") {
            return Ok(self.service.clone());
        }
        if command.contains("json.tool") {
            return match &self.config {
                None => Ok(String::new()),
                Some(v) if v == "invalid" => Err("invalid config.json".to_string()),
                Some(v) => Ok(v.clone()),
            };
        }
        if command.contains("VERSION") {
            return Ok(self.version.clone().unwrap_or_default());
        }
        if command.contains("install-info") {
            return Ok(self.metadata.clone().unwrap_or_default());
        }
        Ok(self.files.clone())
    }
}

fn required_files() -> Vec<String> {
    vec![
        "agent.py".into(),
        "core/__init__.py".into(),
        "core/disk_guardian.py".into(),
        "core/gateway.py".into(),
        "core/issue_worker.py".into(),
        "core/notifier.py".into(),
        "core/session_runner.py".into(),
        "config.json".into(),
    ]
}

fn payload_files() -> Vec<&'static str> {
    vec![
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
    ]
}

fn make_payload(dir: &Path, version: &str) {
    for file in payload_files() {
        let path = dir.join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        if file == "VERSION" {
            fs::write(path, version).unwrap();
        } else {
            fs::write(path, "x").unwrap();
        }
    }
}

#[test]
fn classify_payload_all_partial_none_extra() {
    let all = required_files();
    assert_eq!(deployment::classify_payload(&all), CloudAgentInstallationState::Installed);

    let partial = vec!["agent.py".to_string(), "config.json".to_string()];
    assert_eq!(deployment::classify_payload(&partial), CloudAgentInstallationState::Incomplete);

    let none: Vec<String> = vec![];
    assert_eq!(deployment::classify_payload(&none), CloudAgentInstallationState::NotInstalled);

    let extra = vec!["unrelated.txt".to_string()];
    assert_eq!(deployment::classify_payload(&extra), CloudAgentInstallationState::NotInstalled);

    let all_plus_extra = required_files()
        .into_iter()
        .chain(std::iter::once("extra.txt".to_string()))
        .collect::<Vec<_>>();
    assert_eq!(deployment::classify_payload(&all_plus_extra), CloudAgentInstallationState::Installed);
}

#[test]
fn parse_install_metadata_cases() {
    let (v, t, r) = deployment::parse_install_metadata(
        Some("1.2.3"),
        Some(r#"{"version":"1.2.3","installed_at":"2024-01-01T00:00:00Z"}"#),
    );
    assert_eq!(v.as_deref(), Some("1.2.3"));
    assert_eq!(t.as_deref(), Some("2024-01-01T00:00:00Z"));
    assert!(r.is_none());

    let (v, t, r) = deployment::parse_install_metadata(Some("1.2.3"), None);
    assert_eq!(v.as_deref(), Some("1.2.3"));
    assert!(t.is_none());
    assert!(r.is_none());

    let (v, t, r) = deployment::parse_install_metadata(None, None);
    assert!(v.is_none());
    assert!(t.is_none());
    assert!(r.is_none());

    for metadata in [
        "{bad",
        r#"{"installed_at":"2024-01-01T00:00:00Z"}"#,
        r#"{"version":"9.9.9","installed_at":"2024-01-01T00:00:00Z"}"#,
        r#"{"version":"1.2.3","installed_at":"0"}"#,
        r#"{"version":"1.2.3","installed_at":"-1"}"#,
        r#"{"version":"1.2.3","installed_at":"garbage"}"#,
    ] {
        let (v, t, r) = deployment::parse_install_metadata(Some("1.2.3"), Some(metadata));
        assert_eq!(v.as_deref(), Some("1.2.3"), "remote version preserved for {metadata}");
        assert!(t.is_none(), "invalid metadata must not yield a timestamp for {metadata}");
        assert!(r.is_some(), "invalid metadata must yield a reason for {metadata}");
    }
}

#[test]
fn parse_phase_line_cases() {
    let parsed = deployment::parse_phase_line("ONESPACE_DEPLOY_PHASE phase=platform status=start");
    assert_eq!(
        parsed,
        Some((CloudAgentDeployPhase::Platform, CloudAgentDeployPhaseStatus::Start, None))
    );

    let parsed = deployment::parse_phase_line("ONESPACE_DEPLOY_PHASE phase=metadata status=ok reason=");
    assert_eq!(
        parsed,
        Some((CloudAgentDeployPhase::Metadata, CloudAgentDeployPhaseStatus::Ok, None))
    );

    let parsed = deployment::parse_phase_line("ONESPACE_DEPLOY_PHASE phase=service status=error reason=timeout");
    assert_eq!(
        parsed,
        Some((CloudAgentDeployPhase::Service, CloudAgentDeployPhaseStatus::Error, Some("timeout".to_string())))
    );

    for line in [
        "ONESPACE_DEPLOY_PHASE phase=nope status=start",
        "ONESPACE_DEPLOY_PHASE phase=platform status=nope",
        "ONESPACE_DEPLOY_PHASE phase=platform",
        "hello world",
        "",
    ] {
        assert!(deployment::parse_phase_line(line).is_none(), "rejected: {line}");
    }
}

#[test]
fn inspect_installation_classifications() {
    let required = required_files().join("\n");

    let shell = FakeShell {
        fail: false,
        files: String::new(),
        version: None,
        metadata: None,
        service: r#"{"service_active":false}"#.to_string(),
        config: None,
        commands: Mutex::new(Vec::new()),
    };
    let info = deployment::inspect_installation("host", "/dir", "1.0.0", &shell);
    assert_eq!(info.state, CloudAgentInstallationState::NotInstalled);
    assert_eq!(info.local_latest_version, "1.0.0");

    let shell = FakeShell {
        fail: false,
        files: required.clone(),
        version: Some("1.2.3".to_string()),
        metadata: Some(r#"{"version":"1.2.3","installed_at":"2024-01-01T00:00:00Z"}"#.to_string()),
        service: r#"{"service_active":true}"#.to_string(),
        config: None,
        commands: Mutex::new(Vec::new()),
    };
    let info = deployment::inspect_installation("host", "/dir", "1.0.0", &shell);
    assert_eq!(info.state, CloudAgentInstallationState::Installed);
    assert_eq!(info.remote_version.as_deref(), Some("1.2.3"));
    assert_eq!(info.installed_at.as_deref(), Some("2024-01-01T00:00:00Z"));
    assert_eq!(info.service_active, Some(true));

    let shell = FakeShell {
        fail: false,
        files: "agent.py\nconfig.json".to_string(),
        version: None,
        metadata: None,
        service: r#"{"service_active":false}"#.to_string(),
        config: None,
        commands: Mutex::new(Vec::new()),
    };
    let info = deployment::inspect_installation("host", "/dir", "1.0.0", &shell);
    assert_eq!(info.state, CloudAgentInstallationState::Incomplete);

    let shell = FakeShell {
        fail: false,
        files: "unrelated.txt".to_string(),
        version: None,
        metadata: None,
        service: r#"{"service_active":false}"#.to_string(),
        config: None,
        commands: Mutex::new(Vec::new()),
    };
    let info = deployment::inspect_installation("host", "/dir", "1.0.0", &shell);
    assert_eq!(info.state, CloudAgentInstallationState::ProbeError);
    assert!(info.probe_reason.as_deref().unwrap_or("").contains("conflict"));

    let shell = FakeShell {
        fail: true,
        files: String::new(),
        version: None,
        metadata: None,
        service: String::new(),
        config: None,
        commands: Mutex::new(Vec::new()),
    };
    let info = deployment::inspect_installation("host", "/dir", "1.0.0", &shell);
    assert_eq!(info.state, CloudAgentInstallationState::ProbeError);
    assert_eq!(info.local_latest_version, "1.0.0");

    let shell = FakeShell {
        fail: false,
        files: required.clone(),
        version: Some("1.2.3".to_string()),
        metadata: Some("{bad".to_string()),
        service: r#"{"service_active":false}"#.to_string(),
        config: None,
        commands: Mutex::new(Vec::new()),
    };
    let info = deployment::inspect_installation("host", "/dir", "1.0.0", &shell);
    assert_eq!(info.state, CloudAgentInstallationState::Installed);
    assert!(info.metadata_reason.is_some());
    assert!(info.configuration_error.is_none());

    let shell = FakeShell {
        fail: false,
        files: required.clone(),
        version: Some("1.2.3".to_string()),
        metadata: Some(r#"{"version":"1.2.3","installed_at":"2024-01-01T00:00:00Z"}"#.to_string()),
        service: r#"{"service_active":false}"#.to_string(),
        config: Some("invalid".to_string()),
        commands: Mutex::new(Vec::new()),
    };
    let info = deployment::inspect_installation("host", "/dir", "1.0.0", &shell);
    assert_eq!(info.state, CloudAgentInstallationState::Installed);
    assert!(info.configuration_error.is_some());

    let shell = FakeShell {
        fail: false,
        files: required.clone(),
        version: Some("1.2.3".to_string()),
        metadata: None,
        service: r#"{"service_active":false}"#.to_string(),
        config: None,
        commands: Mutex::new(Vec::new()),
    };
    let info = deployment::inspect_installation("host", "/dir", "1.0.0", &shell);
    assert_eq!(info.remote_version.as_deref(), Some("1.2.3"));
    assert!(info.installed_at.is_none());

    let shell = FakeShell {
        fail: false,
        files: required,
        version: Some("1.2.3".to_string()),
        metadata: Some(r#"{"version":"9.9.9","installed_at":"2024-01-01T00:00:00Z"}"#.to_string()),
        service: r#"{"service_active":false}"#.to_string(),
        config: None,
        commands: Mutex::new(Vec::new()),
    };
    let info = deployment::inspect_installation("host", "/dir", "1.0.0", &shell);
    assert!(info.installed_at.is_none());
    assert!(info.metadata_reason.is_some());
}

#[test]
fn validate_local_payload_and_bundle() {
    let dir = std::env::temp_dir().join(format!("onespace-payload-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    make_payload(&dir, "1.2.3");
    assert_eq!(deployment::validate_local_payload(&dir).unwrap(), "1.2.3");

    fs::remove_file(dir.join("agent.py")).unwrap();
    assert!(deployment::validate_local_payload(&dir).is_err());

    let dir = std::env::temp_dir().join(format!("onespace-payload-empty-version-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    make_payload(&dir, "1.2.3");
    fs::write(dir.join("VERSION"), "").unwrap();
    assert!(deployment::validate_local_payload(&dir).is_err());

    let dir = std::env::temp_dir().join(format!("onespace-payload-ignored-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    make_payload(&dir, "1.2.3");
    fs::write(dir.join("config.json"), "{}").unwrap();
    fs::write(dir.join(".env"), "SECRET=x").unwrap();
    assert_eq!(deployment::validate_local_payload(&dir).unwrap(), "1.2.3");

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let conf_path = manifest_dir.join("tauri.conf.json");
    let raw = fs::read_to_string(&conf_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let resources = v
        .get("bundle")
        .and_then(|b| b.get("resources"))
        .and_then(|r| r.as_object())
        .expect("bundle.resources object");

    let mut expected_owned: HashMap<String, String> = HashMap::new();
    for f in payload_files() {
        expected_owned.insert(
            format!("../server/onespace-cloud-agent/{}", f),
            format!("cloud-agent/{}", f),
        );
    }
    assert_eq!(resources.len(), expected_owned.len());
    for (source, target) in &expected_owned {
        assert_eq!(
            resources.get(source).and_then(|x| x.as_str()),
            Some(target.as_str()),
            "resource mapping for {}",
            source
        );
    }
    for source in resources.keys() {
        assert!(source.starts_with("../server/onespace-cloud-agent/"));
    }
    for target in resources.values().filter_map(|x| x.as_str()) {
        assert!(target.starts_with("cloud-agent/"));
    }
}

#[test]
fn registry_guards() {
    let reg = deployment::DeploymentRegistry::new();
    assert!(reg.begin("host-a".to_string(), "/path-a".to_string(), "op-1".to_string()).is_ok());
    assert!(reg.begin("host-b".to_string(), "/path-b".to_string(), "op-2".to_string()).is_err());
    assert!(reg.begin("host-a".to_string(), "/path-a".to_string(), "op-1".to_string()).is_err());
    reg.finish("op-1");
    reg.finish("unknown");

    reg.record_failed("host-a".to_string(), "/path-a".to_string());
    assert!(reg.may_deploy("host-a", "/path-a", CloudAgentInstallationState::Incomplete).is_ok());
    assert!(reg.may_deploy("host-b", "/path-b", CloudAgentInstallationState::Incomplete).is_err());
    assert!(reg.may_deploy("host-a", "/path-a", CloudAgentInstallationState::Installed).is_ok());
    assert!(reg.may_deploy("host-a", "/path-a", CloudAgentInstallationState::ProbeError).is_err());
    assert!(reg.may_deploy("host-c", "/path-c", CloudAgentInstallationState::NotInstalled).is_ok());

    let fresh = deployment::DeploymentRegistry::new();
    assert!(fresh.may_deploy("host-a", "/path-a", CloudAgentInstallationState::Incomplete).is_err());
    assert!(fresh.may_deploy("host-a", "/path-a", CloudAgentInstallationState::Installed).is_ok());
}

#[test]
fn build_status_command_exact() {
    let cmd = deployment::build_status_command("/root/onespace cloud agent");
    assert_eq!(
        cmd,
        "python3 \"/root/onespace cloud agent/agent.py\" status \"/root/onespace cloud agent/config.json\" --json"
    );
}

#[test]
fn run_deploy_process_success_failure_timeout() {
    let base = std::env::temp_dir().join(format!("onespace-deploy-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&base).unwrap();

    let version = "1.2.3";
    let payload_dir = base.join("payload");
    fs::create_dir_all(&payload_dir).unwrap();
    make_payload(&payload_dir, version);
    let local_version = deployment::validate_local_payload(&payload_dir).unwrap();
    assert_eq!(local_version, version);

    let spaced_dir = base.join("agent dir with spaces");
    fs::create_dir_all(&spaced_dir).unwrap();

    let args_file = base.join("deployer_args.txt");
    let invocations_file = base.join("deployer_invocations.txt");

    let deployer = base.join("deploy.sh");
    let success_script = format!(
        "#!/bin/sh\nprintf 'x' >> '{inv}'\nprintf '%s %s\\n' \"$1\" \"$2\" > '{args}'\nprintf '%s\\n' 'noise line one'\nprintf '%s\\n' 'ONESPACE_DEPLOY_PHASE phase=platform status=start'\nprintf '%s\\n' 'more noise'\nprintf '%s\\n' 'ONESPACE_DEPLOY_PHASE phase=metadata status=ok reason=installed_at=2024-01-01T00:00:00Z'\nexit 0\n",
        inv = invocations_file.display(),
        args = args_file.display(),
    );
    fs::write(&deployer, success_script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&deployer, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let progress = Mutex::new(Vec::<CloudAgentDeploymentProgress>::new());
    let result = deployment::run_deploy_process(
        &deployer,
        &local_version,
        "host",
        spaced_dir.to_str().unwrap(),
        "op-1",
        Duration::from_secs(5),
        |p| progress.lock().unwrap().push(p),
    )
    .unwrap();
    assert!(result.success);
    assert_eq!(result.host, "host");
    assert_eq!(result.operation_id, "op-1");
    assert_eq!(result.phase, CloudAgentDeployPhase::Metadata);
    assert_eq!(result.remote_version.as_deref(), Some(local_version.as_str()));

    let collected = progress.lock().unwrap();
    assert!(!collected.is_empty());
    assert!(collected.iter().all(|p| p.host == "host" && p.operation_id == "op-1"));
    assert!(collected.iter().any(|p| p.phase == CloudAgentDeployPhase::Platform && p.status == CloudAgentDeployPhaseStatus::Start));
    assert_eq!(collected.len(), 2);

    let args = fs::read_to_string(&args_file).unwrap();
    assert_eq!(args.trim(), format!("host {}", spaced_dir.display()));
    let invocations = fs::read_to_string(&invocations_file).unwrap();
    assert_eq!(invocations, "x");

    let failing_deployer = base.join("fail.sh");
    let fail_script = "#!/bin/sh\nprintf '%s\\n' 'ONESPACE_DEPLOY_PHASE phase=upload status=error reason=boom'\nexit 1\n";
    fs::write(&failing_deployer, fail_script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&failing_deployer, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let err = deployment::run_deploy_process(
        &failing_deployer,
        &local_version,
        "host",
        spaced_dir.to_str().unwrap(),
        "op-2",
        Duration::from_secs(5),
        |_| {},
    )
    .unwrap_err();
    assert!(err.contains("boom") || err.contains("upload"));

    let sleepy_deployer = base.join("sleep.sh");
    let sleep_script = "#!/bin/sh\nsleep 5\n";
    fs::write(&sleepy_deployer, sleep_script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&sleepy_deployer, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let started = std::time::Instant::now();
    let err = deployment::run_deploy_process(
        &sleepy_deployer,
        &local_version,
        "host",
        spaced_dir.to_str().unwrap(),
        "op-3",
        Duration::from_millis(100),
        |_| {},
    )
    .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(err.to_lowercase().contains("timeout"));
}

#[test]
fn inspect_records_commands_and_states() {
    let required = required_files().join("\n");

    let shell = FakeShell {
        fail: false,
        files: required.clone(),
        version: Some("1.2.3".to_string()),
        metadata: Some(r#"{"version":"1.2.3","installed_at":"2024-01-01T00:00:00Z"}"#.to_string()),
        service: r#"{"service_active":false}"#.to_string(),
        config: None,
        commands: Mutex::new(Vec::new()),
    };
    let info = deployment::inspect_installation("host", "/dir", "1.0.0", &shell);
    assert_eq!(info.state, CloudAgentInstallationState::Installed);

    let commands = shell.commands.lock().unwrap();
    assert!(commands.iter().any(|c| c.contains("agent.py\" status") && c.contains("--json")));
    let presence = commands
        .iter()
        .find(|c| !c.contains("status") && !c.contains("VERSION") && !c.contains("install-info") && !c.contains("json.tool"))
        .unwrap();
    assert!(!presence.contains("python"));
}

#[test]
fn no_settings_write_during_deploy() {
    let home = std::env::temp_dir().join(format!("onespace-home-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    let _guard = crate::config::test_home::TestHomeGuard::set(&home);

    let base = std::env::temp_dir().join(format!("onespace-deploy-nosettings-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();
    let version = "1.2.3";
    let payload_dir = base.join("payload");
    std::fs::create_dir_all(&payload_dir).unwrap();
    make_payload(&payload_dir, version);
    let local_version = deployment::validate_local_payload(&payload_dir).unwrap();

    let spaced_dir = base.join("agent dir");
    std::fs::create_dir_all(&spaced_dir).unwrap();

    let deployer = base.join("deploy.sh");
    let script = "#!/bin/sh\nprintf '%s\\n' 'ONESPACE_DEPLOY_PHASE phase=platform status=start'\nexit 0\n";
    std::fs::write(&deployer, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&deployer, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let settings_path = crate::cloud_agent::settings::settings_file_path().unwrap();
    let before = std::fs::read(&settings_path).ok();

    let _ = deployment::run_deploy_process(
        &deployer,
        &local_version,
        "host",
        spaced_dir.to_str().unwrap(),
        "op-ns",
        std::time::Duration::from_secs(5),
        |_| {},
    );

    let after = std::fs::read(&settings_path).ok();
    assert_eq!(before, after);
}
