use super::{
    find_default_ssh_keys, password_exists, password_for_tunnel, set_session_timeout,
    ParsedSshAlias, ResolvedAuth, ResolvedSshConfig, SshTunnelAuthKind, SshTunnelForwardMode,
    SshTunnelProbeDraftInput, SshTunnelRecord, SshTunnelSourceKind, SshTunnelUpsertInput,
    SSH_CONNECT_TIMEOUT, SSH_IO_TIMEOUT, SSH_KEEPALIVE_INTERVAL_SECS,
};
use ssh2::{CheckResult, KeyboardInteractivePrompt, KnownHostFileKind, Prompt, Session};
use std::collections::HashMap;
use std::fs;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::PathBuf;

pub(in crate::ssh_tunnels) fn validate_input(
    input: &SshTunnelUpsertInput,
    existing: Option<&SshTunnelRecord>,
) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("Tunnel name is required".to_string());
    }

    match input.source_kind {
        SshTunnelSourceKind::SavedHost => {
            if input
                .saved_host_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err("Please choose an SSH server alias".to_string());
            }
        }
        SshTunnelSourceKind::Custom => {
            let custom = input
                .custom
                .as_ref()
                .ok_or_else(|| "Custom SSH server details are required".to_string())?;
            if custom.host.trim().is_empty() || custom.user.trim().is_empty() {
                return Err("Custom SSH host and username are required".to_string());
            }
            if custom.port == 0 {
                return Err("Custom SSH port is invalid".to_string());
            }
            match custom.auth_kind {
                SshTunnelAuthKind::Password => {
                    let password_supplied = custom
                        .password
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_some();
                    let preserve = custom.preserve_password.unwrap_or(false);
                    let existing_password = existing
                        .map(|record| matches!(record.source_kind, SshTunnelSourceKind::Custom))
                        .unwrap_or(false)
                        && existing
                            .map(|record| password_exists(&record.id))
                            .unwrap_or(false);
                    if !password_supplied && !(preserve && existing_password) {
                        return Err("Password authentication requires a password".to_string());
                    }
                }
                SshTunnelAuthKind::Key => {
                    if custom
                        .key_path
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_none()
                    {
                        return Err("Key authentication requires a key file".to_string());
                    }
                }
            }
        }
    }

    let forward = &input.forward;
    match forward.mode {
        SshTunnelForwardMode::Local => {
            if forward.local_port.unwrap_or(0) == 0 {
                return Err("Local forwarding requires a local port".to_string());
            }
            if forward
                .target_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
                || forward.target_port.unwrap_or(0) == 0
            {
                return Err("Local forwarding requires a target host and target port".to_string());
            }
        }
        SshTunnelForwardMode::Remote => {
            if forward.remote_port.unwrap_or(0) == 0 {
                return Err("Remote forwarding requires a remote port".to_string());
            }
            if forward
                .target_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
                || forward.target_port.unwrap_or(0) == 0
            {
                return Err(
                    "Remote forwarding requires a local target host and target port".to_string(),
                );
            }
        }
        SshTunnelForwardMode::Dynamic => {
            if forward.local_port.unwrap_or(0) == 0 {
                return Err("Dynamic forwarding requires a local SOCKS port".to_string());
            }
            let probe_host = forward
                .dynamic_probe_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string());
            let probe_port = forward.dynamic_probe_port;
            if probe_host.is_some() ^ probe_port.is_some() {
                return Err("Dynamic probe host and port must be provided together".to_string());
            }
        }
    }

    Ok(())
}

pub(in crate::ssh_tunnels) fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.trim_matches('"').to_string()
}

pub(in crate::ssh_tunnels) fn read_ssh_config_sections(
) -> Result<Vec<(Vec<String>, HashMap<String, String>)>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
    let config_path = home.join(".ssh").join("config");
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let mut sections = Vec::new();
    let mut patterns = Vec::new();
    let mut options = HashMap::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else {
            continue;
        };
        let value = parts.collect::<Vec<_>>().join(" ");
        if key.eq_ignore_ascii_case("host") {
            if !patterns.is_empty() || !options.is_empty() {
                sections.push((patterns.clone(), options.clone()));
            }
            patterns = value
                .split_whitespace()
                .map(|item| item.trim().to_string())
                .collect();
            options.clear();
        } else if !patterns.is_empty() {
            let normalized_key = key.to_ascii_lowercase();
            match normalized_key.as_str() {
                "identityfile" | "userknownhostsfile" => {
                    options
                        .entry(normalized_key)
                        .and_modify(|existing: &mut String| {
                            existing.push('\n');
                            existing.push_str(value.trim());
                        })
                        .or_insert_with(|| value.trim().to_string());
                }
                _ => {
                    options.insert(normalized_key, value.trim().to_string());
                }
            }
        }
    }

    if !patterns.is_empty() || !options.is_empty() {
        sections.push((patterns, options));
    }

    Ok(sections)
}

pub(in crate::ssh_tunnels) fn load_saved_host_alias(alias: &str) -> Result<ParsedSshAlias, String> {
    let sections = read_ssh_config_sections()?;
    let mut parsed = ParsedSshAlias::default();

    for (patterns, options) in sections {
        let matched = patterns
            .iter()
            .any(|pattern| pattern == "*" || pattern == alias);
        if !matched {
            continue;
        }
        if let Some(value) = options.get("hostname") {
            parsed.host_name = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("user") {
            parsed.user = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("port") {
            if let Ok(port) = value.trim().parse::<u16>() {
                parsed.port = Some(port);
            }
        }
        if let Some(value) = options.get("identityfile") {
            parsed.identity_files.extend(
                value
                    .lines()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string()),
            );
        }
        if let Some(value) = options.get("identitiesonly") {
            parsed.identities_only = match value.trim().to_ascii_lowercase().as_str() {
                "yes" | "true" | "on" => Some(true),
                "no" | "false" | "off" => Some(false),
                _ => parsed.identities_only,
            };
        }
        if let Some(value) = options.get("hostkeyalias") {
            parsed.host_key_alias = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("userknownhostsfile") {
            parsed.user_known_hosts_file = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("proxycommand") {
            parsed.proxy_command = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("proxyjump") {
            parsed.proxy_jump = Some(value.trim().to_string());
        }
    }

    if parsed.host_name.is_none() {
        return Err(format!(
            "SSH server alias '{}' was not found in ~/.ssh/config",
            alias
        ));
    }
    if parsed.proxy_command.is_some() || parsed.proxy_jump.is_some() {
        return Err(
            "This SSH server alias uses ProxyCommand/ProxyJump and is not supported yet. Please use a custom SSH tunnel instead."
                .to_string(),
        );
    }

    Ok(parsed)
}

pub(in crate::ssh_tunnels) fn resolve_host_key_name(
    requested_host: &str,
    host_name: &str,
    host_key_alias: Option<&str>,
) -> String {
    host_key_alias
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(requested_host)
        .to_string()
        .if_empty_then(host_name)
}

trait StringFallbackExt {
    fn if_empty_then(self, fallback: &str) -> String;
}

impl StringFallbackExt for String {
    fn if_empty_then(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

pub(in crate::ssh_tunnels) fn known_hosts_paths_from_option(
    value: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(vec![known_hosts_path()?]);
    };
    if value.eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    let paths = value
        .split_whitespace()
        .filter_map(|item| {
            let trimmed = item.trim().trim_matches('"');
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
                None
            } else {
                Some(PathBuf::from(expand_tilde(trimmed)))
            }
        })
        .collect::<Vec<_>>();
    if paths.is_empty() {
        Ok(vec![known_hosts_path()?])
    } else {
        Ok(paths)
    }
}

pub(in crate::ssh_tunnels) fn resolved_key_paths(identity_files: &[String]) -> Vec<PathBuf> {
    identity_files
        .iter()
        .map(|path| PathBuf::from(expand_tilde(path)))
        .collect()
}

pub(in crate::ssh_tunnels) fn candidate_saved_host_keys(parsed: &ParsedSshAlias) -> Vec<PathBuf> {
    let mut candidates = resolved_key_paths(&parsed.identity_files);
    if !parsed.identities_only.unwrap_or(false) || candidates.is_empty() {
        for path in find_default_ssh_keys() {
            if !candidates.iter().any(|existing| existing == &path) {
                candidates.push(path);
            }
        }
    }
    candidates
}

pub(in crate::ssh_tunnels) fn resolve_ssh_config_from_record(
    record: &SshTunnelRecord,
) -> Result<ResolvedSshConfig, String> {
    match record.source_kind {
        SshTunnelSourceKind::SavedHost => {
            let alias = record
                .saved_host_name
                .as_deref()
                .ok_or_else(|| "Missing SSH server alias".to_string())?;
            let parsed = load_saved_host_alias(alias)?;
            let host = parsed
                .host_name
                .clone()
                .unwrap_or_else(|| alias.to_string());
            let candidate_keys = candidate_saved_host_keys(&parsed);
            let user = parsed
                .user
                .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "root".to_string()));
            let auth = if !candidate_keys.is_empty() {
                ResolvedAuth::Key {
                    paths: candidate_keys,
                    allow_agent_fallback: !parsed.identities_only.unwrap_or(false),
                }
            } else {
                ResolvedAuth::Agent
            };
            Ok(ResolvedSshConfig {
                host: host.clone(),
                port: parsed.port.unwrap_or(22),
                user,
                auth,
                source_label: alias.to_string(),
                host_key_name: resolve_host_key_name(
                    alias,
                    &host,
                    parsed.host_key_alias.as_deref(),
                ),
                known_hosts_paths: known_hosts_paths_from_option(
                    parsed.user_known_hosts_file.as_deref(),
                )?,
            })
        }
        SshTunnelSourceKind::Custom => {
            let custom = record
                .custom
                .as_ref()
                .ok_or_else(|| "Missing custom SSH configuration".to_string())?;
            let auth = match custom.auth_kind {
                SshTunnelAuthKind::Password => {
                    let password = password_for_tunnel(&record.id)?
                        .ok_or_else(|| "Missing saved password for this SSH tunnel".to_string())?;
                    ResolvedAuth::Password(password)
                }
                SshTunnelAuthKind::Key => ResolvedAuth::Key {
                    paths: vec![PathBuf::from(expand_tilde(
                        custom
                            .key_path
                            .as_deref()
                            .ok_or_else(|| "Missing SSH key path".to_string())?,
                    ))],
                    allow_agent_fallback: false,
                },
            };
            Ok(ResolvedSshConfig {
                host: custom.host.clone(),
                port: custom.port,
                user: custom.user.clone(),
                auth,
                source_label: format!("{}@{}:{}", custom.user, custom.host, custom.port),
                host_key_name: custom.host.clone(),
                known_hosts_paths: known_hosts_paths_from_option(None)?,
            })
        }
    }
}

pub(in crate::ssh_tunnels) fn resolve_ssh_config_from_input(
    input: &SshTunnelProbeDraftInput,
) -> Result<ResolvedSshConfig, String> {
    match input.source_kind {
        SshTunnelSourceKind::SavedHost => {
            let alias = input
                .saved_host_name
                .as_deref()
                .ok_or_else(|| "Missing SSH server alias".to_string())?;
            let parsed = load_saved_host_alias(alias)?;
            let host = parsed
                .host_name
                .clone()
                .unwrap_or_else(|| alias.to_string());
            let candidate_keys = candidate_saved_host_keys(&parsed);
            Ok(ResolvedSshConfig {
                host: host.clone(),
                port: parsed.port.unwrap_or(22),
                user: parsed.user.unwrap_or_else(|| {
                    std::env::var("USER").unwrap_or_else(|_| "root".to_string())
                }),
                auth: if !candidate_keys.is_empty() {
                    ResolvedAuth::Key {
                        paths: candidate_keys,
                        allow_agent_fallback: !parsed.identities_only.unwrap_or(false),
                    }
                } else {
                    ResolvedAuth::Agent
                },
                source_label: alias.to_string(),
                host_key_name: resolve_host_key_name(
                    alias,
                    &host,
                    parsed.host_key_alias.as_deref(),
                ),
                known_hosts_paths: known_hosts_paths_from_option(
                    parsed.user_known_hosts_file.as_deref(),
                )?,
            })
        }
        SshTunnelSourceKind::Custom => {
            let custom = input
                .custom
                .as_ref()
                .ok_or_else(|| "Missing custom SSH configuration".to_string())?;
            let auth = match custom.auth_kind {
                SshTunnelAuthKind::Password => ResolvedAuth::Password(
                    custom
                        .password
                        .clone()
                        .ok_or_else(|| "Password is required for this probe".to_string())?,
                ),
                SshTunnelAuthKind::Key => ResolvedAuth::Key {
                    paths: vec![PathBuf::from(expand_tilde(
                        custom
                            .key_path
                            .as_deref()
                            .ok_or_else(|| "SSH key path is required".to_string())?,
                    ))],
                    allow_agent_fallback: false,
                },
            };
            Ok(ResolvedSshConfig {
                host: custom.host.clone(),
                port: custom.port,
                user: custom.user.clone(),
                auth,
                source_label: format!("{}@{}:{}", custom.user, custom.host, custom.port),
                host_key_name: custom.host.clone(),
                known_hosts_paths: known_hosts_paths_from_option(None)?,
            })
        }
    }
}

pub(in crate::ssh_tunnels) fn known_hosts_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
    let ssh_dir = home.join(".ssh");
    fs::create_dir_all(&ssh_dir).map_err(|e| e.to_string())?;
    Ok(ssh_dir.join("known_hosts"))
}

pub(in crate::ssh_tunnels) fn verify_host_key(
    session: &Session,
    config: &ResolvedSshConfig,
) -> Result<(), String> {
    let (key, key_type) = session
        .host_key()
        .ok_or_else(|| "The SSH server did not provide a host key".to_string())?;
    let mut known_hosts = session.known_hosts().map_err(|e| e.to_string())?;
    for path in &config.known_hosts_paths {
        if path.exists() {
            let _ = known_hosts.read_file(path, KnownHostFileKind::OpenSSH);
        }
    }
    match known_hosts.check_port(&config.host_key_name, config.port, key) {
        CheckResult::Match => Ok(()),
        CheckResult::NotFound => {
            let host_entry = if config.port == 22 {
                config.host_key_name.clone()
            } else {
                format!("[{}]:{}", config.host_key_name, config.port)
            };
            if let Some(path) = config.known_hosts_paths.first() {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                known_hosts
                    .add(&host_entry, key, &host_entry, key_type.into())
                    .map_err(|e| e.to_string())?;
                known_hosts
                    .write_file(path, KnownHostFileKind::OpenSSH)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        CheckResult::Mismatch => Err(format!(
            "Host key mismatch for {}:{}. Please inspect ~/.ssh/known_hosts before retrying.",
            config.host_key_name, config.port
        )),
        CheckResult::Failure => Err("Failed to verify the SSH host key".to_string()),
    }
}

pub(in crate::ssh_tunnels) struct PasswordPrompter {
    password: String,
}

impl KeyboardInteractivePrompt for PasswordPrompter {
    fn prompt<'a>(
        &mut self,
        _username: &str,
        _instructions: &str,
        prompts: &[Prompt<'a>],
    ) -> Vec<String> {
        prompts
            .iter()
            .map(|_prompt| self.password.clone())
            .collect::<Vec<_>>()
    }
}

pub(in crate::ssh_tunnels) fn authenticate_with_agent(
    session: &Session,
    config: &ResolvedSshConfig,
) -> Result<(), String> {
    let mut agent = session.agent().map_err(|e| e.to_string())?;
    agent.connect().map_err(|e| e.to_string())?;
    agent.list_identities().map_err(|e| e.to_string())?;
    let identities = agent.identities().map_err(|e| e.to_string())?;
    let mut authenticated = false;
    for identity in identities {
        if agent.userauth(&config.user, &identity).is_ok() {
            authenticated = true;
            break;
        }
    }
    if authenticated {
        Ok(())
    } else {
        Err(format!(
            "SSH agent authentication failed for '{}'. If this server requires a password, please create the tunnel with Custom SSH instead.",
            config.source_label
        ))
    }
}

pub(in crate::ssh_tunnels) fn authenticate_session(
    session: &Session,
    config: &ResolvedSshConfig,
) -> Result<(), String> {
    match &config.auth {
        ResolvedAuth::Password(password) => {
            if session.userauth_password(&config.user, password).is_err() {
                let mut prompter = PasswordPrompter {
                    password: password.clone(),
                };
                session
                    .userauth_keyboard_interactive(&config.user, &mut prompter)
                    .map_err(|e| e.to_string())?;
            }
        }
        ResolvedAuth::Key {
            paths,
            allow_agent_fallback,
        } => {
            let mut last_error = None;
            for path in paths {
                match session.userauth_pubkey_file(&config.user, None, path, None) {
                    Ok(_) => {
                        last_error = None;
                        break;
                    }
                    Err(error) => {
                        last_error = Some(format!(
                            "Public key authentication failed with {}: {}",
                            path.display(),
                            error
                        ));
                    }
                }
            }
            if !session.authenticated() {
                if *allow_agent_fallback {
                    if authenticate_with_agent(session, config).is_ok() {
                        last_error = None;
                    } else if last_error.is_none() {
                        last_error = Some(
                            "SSH public key authentication failed with all configured identities."
                                .to_string(),
                        );
                    }
                }
            }
            if let Some(error) = last_error {
                return Err(error);
            }
        }
        ResolvedAuth::Agent => authenticate_with_agent(session, config)?,
    }

    if session.authenticated() {
        Ok(())
    } else {
        Err("SSH authentication failed".to_string())
    }
}

pub(in crate::ssh_tunnels) fn open_authenticated_session(
    config: &ResolvedSshConfig,
) -> Result<Session, String> {
    let addr = format!("{}:{}", config.host, config.port);
    let socket_addr = addr
        .parse::<SocketAddr>()
        .ok()
        .or_else(|| {
            (config.host.as_str(), config.port)
                .to_socket_addrs()
                .ok()
                .and_then(|mut addrs| addrs.next())
        })
        .ok_or_else(|| format!("Could not resolve SSH server {}", addr))?;
    let tcp = TcpStream::connect_timeout(&socket_addr, SSH_CONNECT_TIMEOUT)
        .map_err(|e| format!("Failed to connect to SSH server {}: {}", addr, e))?;
    tcp.set_nodelay(true).ok();
    tcp.set_read_timeout(Some(SSH_CONNECT_TIMEOUT)).ok();
    tcp.set_write_timeout(Some(SSH_CONNECT_TIMEOUT)).ok();

    let mut session = Session::new().map_err(|e| e.to_string())?;
    session.set_tcp_stream(tcp);
    set_session_timeout(&session, SSH_CONNECT_TIMEOUT);
    session.handshake().map_err(|e| e.to_string())?;
    verify_host_key(&session, config)?;
    authenticate_session(&session, config)?;
    set_session_timeout(&session, SSH_IO_TIMEOUT);
    session.set_keepalive(true, SSH_KEEPALIVE_INTERVAL_SECS);
    Ok(session)
}
