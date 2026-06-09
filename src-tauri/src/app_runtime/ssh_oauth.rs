use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

#[derive(Serialize, Deserialize)]
pub struct SshHost {
    pub name: String,
    pub host_name: String,
    pub user: String,
    pub port: u16,
}

#[tauri::command]
pub(crate) fn get_ssh_hosts() -> Result<Vec<SshHost>, String> {
    let mut hosts = Vec::new();
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let ssh_config_path = home_dir.join(".ssh").join("config");
    if !ssh_config_path.exists() {
        return Ok(hosts);
    }
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
pub(super) fn connect_ssh(host: &str) -> Result<(), String> {
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
pub(super) fn connect_ssh_custom(
    user: &str,
    host: &str,
    port: u16,
    auth_type: &str,
    auth_val: &str,
) -> Result<(), String> {
    let mut ssh_cmd = format!("ssh -p {} {}@{}", port, user, host);
    if auth_type == "key" && !auth_val.is_empty() {
        ssh_cmd = format!("ssh -i {} -p {} {}@{}", auth_val, port, user, host);
    }
    let script = if auth_type == "password" && !auth_val.is_empty() {
        format!(
            r#"tell application "Terminal"
            activate
            set newTab to do script "{}"
            delay 1.5
            do script "{}" in newTab
        end tell"#,
            ssh_cmd, auth_val
        )
    } else {
        format!(
            r#"tell application "Terminal"
            activate
            do script "{}"
        end tell"#,
            ssh_cmd
        )
    };
    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub(super) async fn exchange_google_token(
    code: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
) -> Result<String, String> {
    let proxy_mgr = crate::proxy::PROXY_MANAGER
        .get()
        .ok_or("Proxy manager not initialized")?;
    let client = proxy_mgr.get_client()?;
    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    res.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(super) async fn refresh_google_token(
    refresh_token: String,
    client_id: String,
    client_secret: String,
) -> Result<String, String> {
    let proxy_mgr = crate::proxy::PROXY_MANAGER
        .get()
        .ok_or("Proxy manager not initialized")?;
    let client = proxy_mgr.get_client()?;
    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("refresh_token", refresh_token.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    res.text().await.map_err(|e| e.to_string())
}
