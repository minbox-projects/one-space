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
    let output = run_version_command(cmd_name);
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let version = if !stdout.is_empty() { stdout } else { stderr };
            CliProbeVersion {
                // Some CLI tools may write version text but still exit with non-zero.
                installed: out.status.success() || !version.is_empty(),
                version,
            }
        }
        Err(_) => CliProbeVersion {
            installed: false,
            version: String::new(),
        },
    }
}

fn run_version_command(cmd_name: &str) -> std::io::Result<Output> {
    let mut cmd = Command::new(cmd_name);
    cmd.arg("--version");
    if let Some(path) = augmented_path() {
        cmd.env("PATH", path);
    }
    cmd.output()
}

fn augmented_path() -> Option<OsString> {
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

        dirs.extend(discover_child_bin_dirs(
            &home.join(".nvm").join("versions").join("node"),
            BinLayout::DirectBin,
        ));
        dirs.extend(discover_child_bin_dirs(
            &home.join(".fnm").join("node-versions"),
            BinLayout::FnmInstallBin,
        ));
    }

    dirs.into_iter().filter(|d| d.is_dir()).collect()
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
