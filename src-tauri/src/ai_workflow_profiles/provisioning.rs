use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::storage::validate_profile_name;
use super::types::ProfileActivationReport;

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
