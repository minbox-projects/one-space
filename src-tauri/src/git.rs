use crate::config::{get_app_dir, StorageConfig};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn get_git_data_dir() -> Result<PathBuf, String> {
    let app_dir = get_app_dir()?;
    Ok(app_dir.join("git_data"))
}

fn prepare_git_command(
    cmd: &str,
    config: &StorageConfig,
    args: &[&str],
    dir: Option<&PathBuf>,
) -> Command {
    let mut command = Command::new("git");

    if let Some(d) = dir {
        command.current_dir(d);
    }

    command.arg(cmd);
    for arg in args {
        command.arg(arg);
    }

    // Set SSH key if using SSH
    if config.auth_method.as_deref() == Some("ssh") {
        if let Some(key_path) = &config.ssh_key_path {
            if !key_path.is_empty() {
                let ssh_cmd = format!("ssh -i \"{}\" -o StrictHostKeyChecking=no", key_path);
                command.env("GIT_SSH_COMMAND", ssh_cmd);
            }
        }
    }

    command
}

fn get_git_url(config: &StorageConfig) -> Result<String, String> {
    let url = config
        .git_url
        .as_ref()
        .ok_or("Git URL not configured")?
        .clone();

    if config.auth_method.as_deref() == Some("http") {
        if let (Some(user), Some(token)) = (&config.http_username, &config.http_token) {
            if !user.is_empty() && !token.is_empty() {
                // Insert credentials into URL if not already present
                if url.starts_with("https://") {
                    let rest = &url["https://".len()..];
                    return Ok(format!("https://{}:{}@{}", user, token, rest));
                } else if url.starts_with("http://") {
                    let rest = &url["http://".len()..];
                    return Ok(format!("http://{}:{}@{}", user, token, rest));
                }
            }
        }
    }

    Ok(url)
}

pub fn init_or_pull_git_repo(config: &StorageConfig) -> Result<(), String> {
    let git_dir = get_git_data_dir()?;
    let url = get_git_url(config)?;

    if git_dir.join(".git").exists() {
        // Update remote URL to ensure latest credentials/URL
        let remote_output = prepare_git_command(
            "remote",
            config,
            &["set-url", "origin", &url],
            Some(&git_dir),
        )
        .output()
        .map_err(|e| e.to_string())?;

        if !remote_output.status.success() {
            let stderr = String::from_utf8_lossy(&remote_output.stderr);
            return Err(format!("Git remote set-url failed: {}", stderr));
        }

        // Pull
        let output = prepare_git_command("pull", config, &[], Some(&git_dir))
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Git pull failed: {}", stderr));
        }
    } else {
        // Clone
        if git_dir.exists() {
            fs::remove_dir_all(&git_dir).unwrap_or_default();
        }

        let output = prepare_git_command("clone", config, &[&url, git_dir.to_str().unwrap()], None)
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Git clone failed: {}", stderr));
        }
    }

    Ok(())
}

pub fn commit_and_push(config: &StorageConfig) -> Result<(), String> {
    let git_dir = get_git_data_dir()?;
    if !git_dir.join(".git").exists() {
        return Ok(());
    }

    // git add .
    prepare_git_command("add", config, &["."], Some(&git_dir))
        .output()
        .map_err(|e| e.to_string())?;

    // git commit -m "Auto sync from OneSpace (hostname)"
    let commit_msg = format!("Auto sync from OneSpace ({})", crate::get_hostname());
    let commit_output = prepare_git_command("commit", config, &["-m", &commit_msg], Some(&git_dir))
        .output()
        .map_err(|e| e.to_string())?;

    // If nothing to commit, output contains "nothing to commit", it's fine.
    // If it succeeded, we push.
    if commit_output.status.success() {
        let push_output = prepare_git_command("push", config, &[], Some(&git_dir))
            .output()
            .map_err(|e| e.to_string())?;

        if !push_output.status.success() {
            let stderr = String::from_utf8_lossy(&push_output.stderr);
            return Err(format!("Git push failed: {}", stderr));
        }
    }

    Ok(())
}

#[tauri::command]
pub fn sync_git() -> Result<(), String> {
    let config = crate::config::get_config()?;
    if config.storage_type == "git" {
        init_or_pull_git_repo(&config)?;
        commit_and_push(&config)?;
    }
    Ok(())
}
