use crate::config::{get_app_dir, StorageConfig};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use tauri::Emitter;

static SYNC_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Clone)]
struct SyncStatusPayload {
    status: String,
    message: Option<String>,
}

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
    let mut command = crate::get_git_command();

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
        // Only update remote URL if it has changed to avoid locking issues
        let current_url_output = prepare_git_command("remote", config, &["get-url", "origin"], Some(&git_dir))
            .output();
        
        let should_set_url = match current_url_output {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim() != url
            }
            _ => true,
        };

        if should_set_url {
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
        }

        // Pull
        let output = prepare_git_command("pull", config, &["origin", "main"], Some(&git_dir))
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
            // If it's a new/empty repo, pull will fail because the remote ref doesn't exist yet.
            // We ignore common "empty remote" error patterns.
            let is_empty_repo_error = stderr.contains("no such ref was fetched")
                || stderr.contains("couldn't find remote ref")
                || stderr.contains("could not read from remote repository")
                || stderr.contains("not a git repository");

            if !is_empty_repo_error {
                return Err(format!(
                    "Git pull failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }
    } else {
        // Clone
        // We only remove .git if it's corrupted, but keep other files
        // Actually, for a clean clone, we should use a temporary directory
        let temp_clone_dir = git_dir.parent().unwrap().join("git_temp_clone");
        if temp_clone_dir.exists() {
            fs::remove_dir_all(&temp_clone_dir).unwrap_or_default();
        }

        let output = prepare_git_command("clone", config, &[&url, temp_clone_dir.to_str().unwrap()], None)
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Git clone failed: {}", stderr));
        }

        // If clone succeeded, move the .git directory to the target git_dir
        if !git_dir.exists() {
            fs::create_dir_all(&git_dir).map_err(|e| e.to_string())?;
        }
        
        let new_git_meta = temp_clone_dir.join(".git");
        let target_git_meta = git_dir.join(".git");
        if target_git_meta.exists() {
            fs::remove_dir_all(&target_git_meta).unwrap_or_default();
        }
        fs::rename(new_git_meta, target_git_meta).map_err(|e| e.to_string())?;
        
        // Also copy files from cloned repo to git_dir if they don't exist locally
        // (This helps merge remote data with local data on first sync)
        for entry in fs::read_dir(&temp_clone_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_file() {
                let dest = git_dir.join(path.file_name().unwrap());
                if !dest.exists() {
                    fs::copy(&path, &dest).map_err(|e| e.to_string())?;
                }
            } else if path.is_dir() && path.file_name().unwrap() != ".git" {
                // For subdirectories (hostnames), we merge them
                let dir_name = path.file_name().unwrap();
                let dest_dir = git_dir.join(dir_name);
                if !dest_dir.exists() {
                    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
                }
                for sub_entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
                    let sub_entry = sub_entry.map_err(|e| e.to_string())?;
                    let sub_path = sub_entry.path();
                    if sub_path.is_file() {
                        let sub_dest = dest_dir.join(sub_path.file_name().unwrap());
                        if !sub_dest.exists() {
                            fs::copy(&sub_path, &sub_dest).map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
        }
        
        fs::remove_dir_all(&temp_clone_dir).unwrap_or_default();
    }

    Ok(())
}

pub fn commit_and_push(config: &StorageConfig) -> Result<(), String> {
    let git_dir = get_git_data_dir()?;
    if !git_dir.join(".git").exists() {
        return Ok(());
    }

    // Ensure user identity is set locally for this repo if not set globally
    // This avoids "Author identity unknown" errors on fresh systems
    let _ = prepare_git_command(
        "config",
        config,
        &["user.name", "OneSpace Auto Sync"],
        Some(&git_dir),
    )
    .output();
    let _ = prepare_git_command(
        "config",
        config,
        &["user.email", "sync@onespace.ai"],
        Some(&git_dir),
    )
    .output();

    // git add .
    let add_output = prepare_git_command("add", config, &["."], Some(&git_dir))
        .output()
        .map_err(|e| e.to_string())?;

    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        return Err(format!("Git add failed: {}", stderr));
    }

    // git commit -m "Auto sync from OneSpace (hostname)"
    let commit_msg = format!("Auto sync from OneSpace ({})", crate::get_hostname());
    let _commit_output = prepare_git_command("commit", config, &["-m", &commit_msg], Some(&git_dir))
        .output()
        .map_err(|e| e.to_string())?;

    // If it succeeded OR if there was nothing to commit, we try to push anyway
    // (push might be needed for other changes or just to stay in sync)
    // Note: git commit returns non-zero if there's nothing to commit.

    let push_output = prepare_git_command("push", config, &["origin", "main"], Some(&git_dir))
        .output()
        .map_err(|e| e.to_string())?;

    if !push_output.status.success() {
        let stderr = String::from_utf8_lossy(&push_output.stderr);
        // If it's just "nothing to commit" during push, it's fine,
        // but push usually fails for network/auth/upstream changes.
        if !stderr.contains("Everything up-to-date") && !stderr.is_empty() {
            return Err(format!("Git push failed: {}", stderr));
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn sync_git(app: tauri::AppHandle) -> Result<(), String> {
    let config = crate::config::get_config()?;
    if config.storage_type == "git" {
        let _ = app.emit(
            "git-sync-status",
            SyncStatusPayload {
                status: "syncing".to_string(),
                message: None,
            },
        );

        // Run sync in a blocking thread to avoid any chance of UI stutter
        let res = tauri::async_runtime::spawn_blocking(move || {
            let _lock = SYNC_LOCK.lock().map_err(|e| e.to_string())?;
            match (|| {
                init_or_pull_git_repo(&config)?;
                commit_and_push(&config)?;
                Ok::<(), String>(())
            })() {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            }
        }).await.map_err(|e| e.to_string())?;

        match res {
            Ok(_) => {
                let _ = app.emit(
                    "git-sync-status",
                    SyncStatusPayload {
                        status: "success".to_string(),
                        message: None,
                    },
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "git-sync-status",
                    SyncStatusPayload {
                        status: "error".to_string(),
                        message: Some(e.clone()),
                    },
                );
                return Err(e);
            }
        }
    }
    Ok(())
}
