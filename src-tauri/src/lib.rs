use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn get_data_dir() -> Result<PathBuf, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let data_dir = home_dir.join(".config").join("onespace").join("data");

    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    }

    Ok(data_dir)
}

#[tauri::command]
fn read_snippets() -> Result<String, String> {
    let data_dir = get_data_dir()?;
    let snippets_path = data_dir.join("snippets.json");

    if !snippets_path.exists() {
        // Return empty array as string if file doesn't exist
        return Ok("[]".to_string());
    }

    fs::read_to_string(snippets_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_snippets(snippets_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let snippets_path = data_dir.join("snippets.json");

    let mut file = File::create(snippets_path).map_err(|e| e.to_string())?;
    file.write_all(snippets_json.as_bytes())
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn read_bookmarks() -> Result<String, String> {
    let data_dir = get_data_dir()?;
    let bookmarks_path = data_dir.join("bookmarks.json");

    if !bookmarks_path.exists() {
        return Ok("[]".to_string());
    }

    fs::read_to_string(bookmarks_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_bookmarks(bookmarks_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let bookmarks_path = data_dir.join("bookmarks.json");

    let mut file = File::create(bookmarks_path).map_err(|e| e.to_string())?;
    file.write_all(bookmarks_json.as_bytes())
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct SshHost {
    pub name: String,
    pub host_name: String,
    pub user: String,
    pub port: u16,
}

#[tauri::command]
fn get_ssh_hosts() -> Result<Vec<SshHost>, String> {
    let mut hosts = Vec::new();

    // Get home directory
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let ssh_config_path = home_dir.join(".ssh").join("config");

    if !ssh_config_path.exists() {
        return Ok(hosts); // Return empty list if no config
    }

    // Manual parsing since ssh_cfg crate might be too strict
    if let Ok(content) = fs::read_to_string(&ssh_config_path) {
        let mut current_host: Option<String> = None;
        let mut current_hostname = String::new();
        let mut current_user = String::new();
        let mut current_port = 22;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let key = parts[0].to_lowercase();

            if key == "host" && parts.len() > 1 {
                // If we were building a host, save it (skip wildcard hosts like '*')
                if let Some(name) = current_host.take() {
                    if name != "*" {
                        hosts.push(SshHost {
                            name,
                            host_name: if current_hostname.is_empty() {
                                "Unknown".to_string()
                            } else {
                                current_hostname.clone()
                            },
                            user: if current_user.is_empty() {
                                "root".to_string()
                            } else {
                                current_user.clone()
                            },
                            port: current_port,
                        });
                    }
                }

                // Start new host
                current_host = Some(parts[1].to_string());
                current_hostname.clear();
                current_user.clear();
                current_port = 22;
            } else if key == "hostname" && parts.len() > 1 && current_host.is_some() {
                current_hostname = parts[1].to_string();
            } else if key == "user" && parts.len() > 1 && current_host.is_some() {
                current_user = parts[1].to_string();
            } else if key == "port" && parts.len() > 1 && current_host.is_some() {
                if let Ok(port) = parts[1].parse::<u16>() {
                    current_port = port;
                }
            }
        }

        // Don't forget the last host
        if let Some(name) = current_host {
            if name != "*" {
                hosts.push(SshHost {
                    name,
                    host_name: if current_hostname.is_empty() {
                        "Unknown".to_string()
                    } else {
                        current_hostname.clone()
                    },
                    user: if current_user.is_empty() {
                        "root".to_string()
                    } else {
                        current_user.clone()
                    },
                    port: current_port,
                });
            }
        }
    }

    Ok(hosts)
}

#[tauri::command]
fn connect_ssh(host: &str) -> Result<(), String> {
    // Open a new Terminal window and run the SSH command
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "ssh {}"
        end tell"#,
        host
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn connect_ssh_custom(
    user: &str,
    host: &str,
    port: u16,
    auth_type: &str,
    auth_val: &str,
) -> Result<(), String> {
    // Build the base SSH command
    let mut ssh_cmd = format!("ssh -p {} {}@{}", port, user, host);

    // If using identity file, append it
    if auth_type == "key" && !auth_val.is_empty() {
        ssh_cmd = format!("ssh -i {} -p {} {}@{}", auth_val, port, user, host);
    }

    let script;

    // If using password, we need a slightly more complex AppleScript or sshpass
    // For simplicity and macOS compatibility, we'll try to just paste the password
    // or rely on user typing it if it's too complex to inject cleanly without sshpass.
    // A safe approach for Terminal.app without installing external tools:
    if auth_type == "password" && !auth_val.is_empty() {
        script = format!(
            r#"tell application "Terminal"
                activate
                set newTab to do script "{}"
                delay 1.5
                do script "{}" in newTab
            end tell"#,
            ssh_cmd, auth_val
        );
    } else {
        script = format!(
            r#"tell application "Terminal"
                activate
                do script "{}"
            end tell"#,
            ssh_cmd
        );
    }

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn read_notes() -> Result<String, String> {
    let data_dir = get_data_dir()?;
    let notes_path = data_dir.join("notes.json");

    if !notes_path.exists() {
        return Ok("[]".to_string());
    }

    fs::read_to_string(notes_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_notes(notes_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let notes_path = data_dir.join("notes.json");

    let mut file = File::create(notes_path).map_err(|e| e.to_string())?;
    file.write_all(notes_json.as_bytes())
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct TmuxSession {
    pub name: String,
    pub created: u64,
    pub attached: bool,
    pub path: String,
}

#[tauri::command]
fn get_tmux_sessions() -> Result<Vec<TmuxSession>, String> {
    let output = Command::new("tmux")
        .arg("ls")
        .arg("-F")
        .arg("#{session_name}|#{session_created}|#{session_attached}|#{pane_current_path}")
        .output();

    match output {
        Ok(out) => {
            if !out.status.success() {
                // Usually means no server running or no sessions
                return Ok(vec![]);
            }

            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut sessions = Vec::new();

            for line in stdout.lines() {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() == 4 {
                    sessions.push(TmuxSession {
                        name: parts[0].to_string(),
                        created: parts[1].parse().unwrap_or(0),
                        attached: parts[2] != "0",
                        path: parts[3].to_string(),
                    });
                }
            }

            // Sort by created time descending
            sessions.sort_by(|a, b| b.created.cmp(&a.created));
            Ok(sessions)
        }
        Err(e) => Err(format!("Failed to execute tmux: {}", e)),
    }
}

#[tauri::command]
fn create_tmux_session(session_name: &str, working_dir: &str, command: &str) -> Result<(), String> {
    // Create the session in the background
    let mut args = vec!["new-session", "-d", "-s", session_name, "-c", working_dir];
    if !command.is_empty() {
        args.push(command);
    }

    let status = Command::new("tmux")
        .args(&args)
        .status()
        .map_err(|e| e.to_string())?;

    if !status.success() {
        return Err("Failed to create tmux session. Please check if the command is valid.".into());
    }

    Ok(())
}

#[tauri::command]
fn attach_tmux_session(session_name: &str) -> Result<(), String> {
    // Use AppleScript to open Mac Terminal and attach to the session
    // We open a new window and execute the attach command
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "tmux attach -t {}"
        end tell"#,
        session_name
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn kill_tmux_session(session_name: &str) -> Result<(), String> {
    let status = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .status()
        .map_err(|e| e.to_string())?;

    if !status.success() {
        return Err("Failed to kill session.".into());
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_tmux_sessions,
            create_tmux_session,
            attach_tmux_session,
            kill_tmux_session,
            get_ssh_hosts,
            connect_ssh,
            connect_ssh_custom,
            read_snippets,
            save_snippets,
            read_bookmarks,
            save_bookmarks,
            read_notes,
            save_notes
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
