use regex::Regex;
use std::cmp::Ordering;
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
/// Handles: `v1.2.3`, `tool 1.2.3`, `1.2.3-beta.1`, `1.2.3+build.7`, plain `1.2.3`.
pub fn extract_semver(raw: &str) -> Option<String> {
    let re =
        Regex::new(r"(?i)v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z_.-]+)?(?:\+[0-9A-Za-z_.-]+)?)").ok()?;
    re.captures(raw)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

#[derive(PartialEq, Eq)]
enum PrereleaseIdentifier {
    Numeric(u64),
    Alphanumeric(String),
}

impl Ord for PrereleaseIdentifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Numeric(left), Self::Numeric(right)) => left.cmp(right),
            (Self::Numeric(_), Self::Alphanumeric(_)) => Ordering::Less,
            (Self::Alphanumeric(_), Self::Numeric(_)) => Ordering::Greater,
            (Self::Alphanumeric(left), Self::Alphanumeric(right)) => left.cmp(right),
        }
    }
}

impl PartialOrd for PrereleaseIdentifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(PartialEq, Eq)]
struct VersionKey {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Option<Vec<PrereleaseIdentifier>>,
}

impl Ord for VersionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| {
                compare_prerelease(self.prerelease.as_deref(), other.prerelease.as_deref())
            })
    }
}

impl PartialOrd for VersionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_prerelease(
    left: Option<&[PrereleaseIdentifier]>,
    right: Option<&[PrereleaseIdentifier]>,
) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => {
            for (left_identifier, right_identifier) in left.iter().zip(right.iter()) {
                let ordering = left_identifier.cmp(right_identifier);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            left.len().cmp(&right.len())
        }
    }
}

fn version_key(version: &str) -> Option<VersionKey> {
    let semver = extract_semver(version)?;
    let without_build = semver.split('+').next().unwrap_or(&semver);
    let (core, prerelease) = without_build.split_once('-').unwrap_or((without_build, ""));
    let prerelease = if prerelease.is_empty() {
        None
    } else {
        Some(
            prerelease
                .split('.')
                .map(|identifier| {
                    identifier
                        .parse::<u64>()
                        .map(PrereleaseIdentifier::Numeric)
                        .unwrap_or_else(|_| {
                            PrereleaseIdentifier::Alphanumeric(identifier.to_string())
                        })
                })
                .collect::<Vec<_>>(),
        )
    };
    let mut parts = core.split('.');
    Some(VersionKey {
        major: parts.next()?.parse().ok()?,
        minor: parts.next()?.parse().ok()?,
        patch: parts.next()?.parse().ok()?,
        prerelease,
    })
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

    #[cfg(unix)]
    static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(unix)]
    fn lock_path() -> std::sync::MutexGuard<'static, ()> {
        PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(unix)]
    fn write_cli_version_fixture(dir: &Path, executable: &str, label: &str, version: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::create_dir_all(dir).expect("create CLI fixture directory");
        let path = dir.join(executable);
        fs::write(&path, format!("#!/bin/sh\nprintf '{label} {version}\\n'\n"))
            .expect("write CLI fixture");
        let mut permissions = fs::metadata(&path).expect("read CLI fixture").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("make CLI fixture executable");
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
        let _path_lock = lock_path();

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
        write_cli_version_fixture(&old_dir, "fixture-cli", "codex-cli", "0.145.0");
        write_cli_version_fixture(&new_dir, "fixture-cli", "codex-cli", "0.149.0");

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

    #[cfg(unix)]
    #[test]
    fn probe_cli_version_prefers_opencode_v2_over_v1() {
        let _path_lock = lock_path();

        let root = std::env::temp_dir().join(format!(
            "onespace-opencode-probe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let v1_dir = root.join("v1");
        let v2_dir = root.join("v2");
        write_cli_version_fixture(&v1_dir, "fixture-opencode", "opencode", "1.9.9");
        write_cli_version_fixture(&v2_dir, "fixture-opencode", "opencode", "2.0.0");

        let path_guard = TestPath {
            previous: std::env::var_os("PATH"),
            root: root.clone(),
        };
        std::env::set_var(
            "PATH",
            std::env::join_paths([v1_dir.as_path(), v2_dir.as_path()])
                .expect("build OpenCode fixture PATH"),
        );

        let result = probe_cli_version("fixture-opencode");

        drop(path_guard);
        assert!(result.installed);
        assert_eq!(result.version, "2.0.0");
    }

    #[test]
    fn test_extract_semver_keeps_build_metadata() {
        assert_eq!(
            extract_semver("opencode 2.0.0-beta.10+build.7"),
            Some("2.0.0-beta.10+build.7".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn probe_cli_version_orders_opencode_prerelease_identifiers_numerically() {
        let _path_lock = lock_path();

        let root = std::env::temp_dir().join(format!(
            "onespace-opencode-prerelease-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let beta_2_dir = root.join("beta-2");
        let beta_10_dir = root.join("beta-10");
        write_cli_version_fixture(&beta_2_dir, "fixture-opencode", "opencode", "2.0.0-beta.2");
        write_cli_version_fixture(
            &beta_10_dir,
            "fixture-opencode",
            "opencode",
            "2.0.0-beta.10+build.7",
        );

        let path_guard = TestPath {
            previous: std::env::var_os("PATH"),
            root: root.clone(),
        };
        std::env::set_var(
            "PATH",
            std::env::join_paths([beta_2_dir.as_path(), beta_10_dir.as_path()])
                .expect("build OpenCode fixture PATH"),
        );

        let result = probe_cli_version("fixture-opencode");

        drop(path_guard);
        assert!(result.installed);
        assert_eq!(result.version, "2.0.0-beta.10+build.7");
    }

    #[cfg(unix)]
    #[test]
    fn probe_cli_version_prefers_opencode_stable_over_prerelease() {
        let _path_lock = lock_path();

        let root = std::env::temp_dir().join(format!(
            "onespace-opencode-stable-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let prerelease_dir = root.join("prerelease");
        let stable_dir = root.join("stable");
        write_cli_version_fixture(
            &prerelease_dir,
            "fixture-opencode",
            "opencode",
            "2.0.0-beta.10",
        );
        write_cli_version_fixture(&stable_dir, "fixture-opencode", "opencode", "2.0.0");

        let path_guard = TestPath {
            previous: std::env::var_os("PATH"),
            root: root.clone(),
        };
        std::env::set_var(
            "PATH",
            std::env::join_paths([prerelease_dir.as_path(), stable_dir.as_path()])
                .expect("build OpenCode fixture PATH"),
        );

        let result = probe_cli_version("fixture-opencode");

        drop(path_guard);
        assert!(result.installed);
        assert_eq!(result.version, "2.0.0");
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
