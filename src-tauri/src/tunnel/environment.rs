use std::path::{Path, PathBuf};
use std::process::Command;
use crate::config::get_app_dir;
use super::types::{EnvironmentStatus, TunnelMode};

pub fn binary_name_for(mode: TunnelMode) -> &'static str {
    match mode {
        TunnelMode::Cloudflare => {
            #[cfg(target_os = "windows")]
            { "cloudflared.exe" }
            #[cfg(not(target_os = "windows"))]
            { "cloudflared" }
        }
        TunnelMode::Frp => {
            #[cfg(target_os = "windows")]
            { "frpc.exe" }
            #[cfg(not(target_os = "windows"))]
            { "frpc" }
        }
    }
}

pub fn recommended_command_for(mode: TunnelMode) -> String {
    #[cfg(target_os = "macos")]
    {
        match mode {
            TunnelMode::Cloudflare => "brew install cloudflared".to_string(),
            TunnelMode::Frp => "brew install frpc".to_string(),
        }
    }
    #[cfg(target_os = "windows")]
    {
        match mode {
            TunnelMode::Cloudflare => "winget install --id Cloudflare.cloudflared".to_string(),
            TunnelMode::Frp => "scoop install frp".to_string(),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        match mode {
            TunnelMode::Cloudflare => "https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/".to_string(),
            TunnelMode::Frp => "https://github.com/fatedier/frp/releases".to_string(),
        }
    }
}

pub fn find_binary(mode: TunnelMode) -> Option<PathBuf> {
    let name = binary_name_for(mode);

    // 1. Check application bin directory in active profile
    if let Ok(app_dir) = get_app_dir() {
        let app_bin = app_dir.join("bin").join(name);
        if app_bin.is_file() {
            return Some(app_bin);
        }
    }

    // 2. Common macOS homebrew / user bin paths that might not be in GUI PATH
    #[cfg(target_os = "macos")]
    {
        let standard_paths = [
            Path::new("/opt/homebrew/bin"),
            Path::new("/usr/local/bin"),
            Path::new("/usr/bin"),
        ];
        for dir in standard_paths {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // 3. Check system PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

pub fn get_binary_version(binary_path: &Path) -> Option<String> {
    let output = Command::new(binary_path)
        .arg("--version")
        .output()
        .ok()?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        let first_line = text.lines().next().unwrap_or("").trim();
        if !first_line.is_empty() {
            return Some(first_line.to_string());
        }
    }

    None
}

pub fn check_environment(mode: TunnelMode) -> EnvironmentStatus {
    let binary = find_binary(mode);
    let installed = binary.is_some();
    let binary_path = binary.as_ref().map(|p| p.to_string_lossy().to_string());
    let version = binary.as_deref().and_then(get_binary_version);

    EnvironmentStatus {
        mode,
        installed,
        binary_path,
        version,
        recommended_command: recommended_command_for(mode),
    }
}
