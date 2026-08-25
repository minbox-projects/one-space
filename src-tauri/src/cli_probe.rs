use regex::Regex;
use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Debug, Clone)]
pub struct CliProbeVersion {
    pub installed: bool,
    pub version: String,
}

pub fn probe_cli_version(cmd_name: &str) -> CliProbeVersion {
    let mut fallback = None;
    let mut newest = None;

    for command_path in command_candidates(cmd_name) {
        let Ok(out) = run_version_command(&command_path) else {
            continue;
        };
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let raw_version = if !stdout.is_empty() { stdout } else { stderr };
        if raw_version.is_empty() {
            continue;
        }

        let candidate = CliProbeVersion {
            // Some CLI tools may write version text but still exit with non-zero.
            installed: out.status.success() || !raw_version.is_empty(),
            version: extract_semver(&raw_version).unwrap_or(raw_version),
        };
        if fallback.is_none() {
            fallback = Some(candidate.clone());
        }
        if let Some(key) = version_key(&candidate.version) {
            let should_replace = newest
                .as_ref()
                .map(|(_, current_key)| key > *current_key)
                .unwrap_or(true);
            if should_replace {
                newest = Some((candidate, key));
            }
        }
    }

    newest
        .map(|(probe, _)| probe)
        .or(fallback)
        .unwrap_or(CliProbeVersion {
            installed: false,
            version: String::new(),
        })
}

/// Extract the first semver (x.y.z) from raw CLI --version output.
/// Handles: `v1.2.3`, `tool 1.2.3`, `1.2.3-beta.1`, plain `1.2.3`.
pub fn extract_semver(raw: &str) -> Option<String> {
    let re = Regex::new(r"(?i)v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z_.-]+)?)").ok()?;
    re.captures(raw)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn version_key(version: &str) -> Option<(u64, u64, u64, bool)> {
    let semver = extract_semver(version)?;
    let (core, prerelease) = semver.split_once('-').unwrap_or((&semver, ""));
    let mut parts = core.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        prerelease.is_empty(),
    ))
}

fn command_candidates(cmd_name: &str) -> Vec<PathBuf> {
    let command_path = Path::new(cmd_name);
    if command_path.components().count() > 1 {
        return vec![command_path.to_path_buf()];
    }

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    if let Some(path_os) = augmented_path() {
        for dir in env::split_paths(&path_os) {
            let candidate = dir.join(cmd_name);
            if candidate.is_file() && seen.insert(candidate.clone()) {
                candidates.push(candidate);
            }
        }
    }
    if candidates.is_empty() {
        candidates.push(command_path.to_path_buf());
    }
    candidates
}

fn run_version_command(command_path: &Path) -> std::io::Result<Output> {
    let mut cmd = Command::new(command_path);
    cmd.arg("--version");
    if let Some(path) = augmented_path() {
        cmd.env("PATH", path);
    }
    cmd.output()
}

pub(crate) fn augmented_path() -> Option<OsString> {
    let mut merged = Vec::<PathBuf>::new();
    let mut seen = HashSet::<PathBuf>::new();

    if let Some(path_os) = env::var_os("PATH") {
        for dir in env::split_paths(&path_os) {
            if seen.insert(dir.clone()) {
                merged.push(dir);
            }
        }
    }

    for dir in extra_cli_bin_dirs() {
        if seen.insert(dir.clone()) {
            merged.push(dir);
        }
    }

    env::join_paths(merged).ok()
}

fn extra_cli_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ];

    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".npm-global").join("bin"));
        dirs.push(home.join(".volta").join("bin"));
        dirs.push(home.join(".bun").join("bin"));
        dirs.push(home.join(".asdf").join("shims"));
        dirs.push(home.join(".local").join("share").join("mise").join("shims"));
        dirs.push(home.join(".pnpm"));
        dirs.push(home.join(".pnpm").join("bin"));
        dirs.push(home.join(".opencode").join("bin"));

        dirs.extend(discover_child_bin_dirs(
            &home.join(".nvm").join("versions").join("node"),
            BinLayout::DirectBin,
        ));
        for root in fnm_node_version_roots(&home) {
            dirs.extend(discover_child_bin_dirs(&root, BinLayout::FnmInstallBin));
        }
    }

    dirs.into_iter().filter(|d| d.is_dir()).collect()
}

fn fnm_node_version_roots(home: &Path) -> [PathBuf; 2] {
    [
        home.join(".fnm").join("node-versions"),
        home.join(".local")
            .join("share")
            .join("fnm")
            .join("node-versions"),
    ]
}

enum BinLayout {
    DirectBin,
    FnmInstallBin,
}

fn discover_child_bin_dirs(root: &Path, layout: BinLayout) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut children: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();

    children.sort();
    children.reverse();

    children
        .into_iter()
        .map(|child| match layout {
            BinLayout::DirectBin => child.join("bin"),
            BinLayout::FnmInstallBin => child.join("installation").join("bin"),
        })
        .filter(|path| path.is_dir())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{extract_semver, fnm_node_version_roots, probe_cli_version};
    use std::ffi::OsString;
    use std::fs;
    use std::path::Path;

    #[cfg(unix)]
    struct TestPath {
        previous: Option<OsString>,
        root: std::path::PathBuf,
    }

    #[cfg(unix)]
    impl Drop for TestPath {
        fn drop(&mut self) {
            if let Some(path) = self.previous.take() {
                std::env::set_var("PATH", path);
            } else {
                std::env::remove_var("PATH");
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn test_extract_semver_pure() {
        assert_eq!(extract_semver("1.2.3"), Some("1.2.3".to_string()));
    }

    #[test]
    fn test_extract_semver_with_v_prefix() {
        assert_eq!(extract_semver("v1.2.3"), Some("1.2.3".to_string()));
    }

    #[test]
    fn test_extract_semver_with_tool_name() {
        assert_eq!(extract_semver("claude 1.2.3"), Some("1.2.3".to_string()));
    }

    #[test]
    fn test_extract_semver_with_prerelease() {
        assert_eq!(
            extract_semver("1.2.3-beta.1"),
            Some("1.2.3-beta.1".to_string())
        );
    }

    #[test]
    fn test_extract_semver_no_version() {
        assert_eq!(extract_semver("no version here"), None);
    }

    #[test]
    fn test_extract_semver_empty() {
        assert_eq!(extract_semver(""), None);
    }

    #[cfg(unix)]
    #[test]
    fn probe_cli_version_uses_the_newest_installed_copy() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "onespace-cli-probe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let old_dir = root.join("old");
        let new_dir = root.join("new");
        fs::create_dir_all(&old_dir).expect("create old CLI directory");
        fs::create_dir_all(&new_dir).expect("create new CLI directory");
        for (dir, version) in [(&old_dir, "0.145.0"), (&new_dir, "0.149.0")] {
            let path = dir.join("fixture-cli");
            fs::write(
                &path,
                format!("#!/bin/sh\nprintf 'codex-cli {version}\\n'\n"),
            )
            .expect("write CLI fixture");
            let mut permissions = fs::metadata(&path).expect("read CLI fixture").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("make CLI fixture executable");
        }

        let path_guard = TestPath {
            previous: std::env::var_os("PATH"),
            root: root.clone(),
        };
        std::env::set_var(
            "PATH",
            std::env::join_paths([old_dir.as_path(), new_dir.as_path()])
                .expect("build fixture PATH"),
        );

        let result = probe_cli_version("fixture-cli");

        drop(path_guard);
        assert!(result.installed);
        assert_eq!(result.version, "0.149.0");
    }

    #[test]
    fn test_fnm_node_version_roots_supports_legacy_and_xdg_layouts() {
        assert_eq!(
            fnm_node_version_roots(Path::new("/example/home")),
            [
                Path::new("/example/home/.fnm/node-versions").to_path_buf(),
                Path::new("/example/home/.local/share/fnm/node-versions").to_path_buf(),
            ]
        );
    }
}
