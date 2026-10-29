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

#[cfg(test)]
static GOOGLE_TOKEN_BASE_OVERRIDE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
#[cfg(test)]
pub fn set_google_token_base_override(v: Option<String>) {
    *GOOGLE_TOKEN_BASE_OVERRIDE.lock().unwrap() = v;
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
    let token_url = {
        #[cfg(test)]
        { GOOGLE_TOKEN_BASE_OVERRIDE.lock().unwrap().clone().unwrap_or_else(|| "https://oauth2.googleapis.com/token".to_string()) }
        #[cfg(not(test))]
        { "https://oauth2.googleapis.com/token".to_string() }
    };
    let res = client
        .post(&token_url)
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
    let token_url = {
        #[cfg(test)]
        { GOOGLE_TOKEN_BASE_OVERRIDE.lock().unwrap().clone().unwrap_or_else(|| "https://oauth2.googleapis.com/token".to_string()) }
        #[cfg(not(test))]
        { "https://oauth2.googleapis.com/token".to_string() }
    };
    let res = client
        .post(&token_url)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::test_support::{SERIAL_GUARD, with_global_proxy, HttpProxyRecorder};

    #[tokio::test]
    async fn app_runtime_ssh_oauth_proxy_switch_exchange_and_refresh() {
        let _g = SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());

        let a = HttpProxyRecorder::new("tokA").await;
        let b = HttpProxyRecorder::new("tokB").await;

        let a_cfg = crate::config::ProxyConfig {
            proxy_enabled: true, proxy_type: "http".to_string(), proxy_host: "127.0.0.1".to_string(),
            proxy_port: a.url().split(':').last().unwrap().parse().unwrap(),
            proxy_username: None, proxy_password: None, check_interval: 15,
        };
        let _guard = with_global_proxy(Some(a_cfg));
        set_google_token_base_override(Some(a.url()));
        let _ = exchange_google_token("code".into(), "cid".into(), "secret".into(), "http://cb".into()).await;
        let _ = refresh_google_token("rt".into(), "cid".into(), "secret".into()).await;
        // target path is token_url path "/"? recorder records first request target which is full URL path. a.url() has no path, POST to "http://host:port" -> target "/" maybe. Let's not assert exact path; assert hits.
        assert_eq!(a.hits(), 2);

        drop(_guard);
        let b_cfg = crate::config::ProxyConfig {
            proxy_enabled: true, proxy_type: "http".to_string(), proxy_host: "127.0.0.1".to_string(),
            proxy_port: b.url().split(':').last().unwrap().parse().unwrap(),
            proxy_username: None, proxy_password: None, check_interval: 15,
        };
        let _guard2 = with_global_proxy(Some(b_cfg));
        set_google_token_base_override(Some(b.url()));
        let _ = exchange_google_token("code".into(), "cid".into(), "secret".into(), "http://cb".into()).await;
        let _ = refresh_google_token("rt".into(), "cid".into(), "secret".into()).await;
        assert_eq!(a.hits(), 2);
        assert_eq!(b.hits(), 2);

        // body/form contains grant_type: check recorded requests
        let reqs = a.requests();
        assert!(reqs.iter().any(|r| r.contains("grant_type=authorization_code")));
        assert!(reqs.iter().any(|r| r.contains("grant_type=refresh_token")));

        set_google_token_base_override(None);
    }
}
