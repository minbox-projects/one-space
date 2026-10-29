use std::process::Command;
use std::sync::OnceLock;

#[cfg(test)]
static GIT_COMMAND_OVERRIDE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[cfg(test)]
pub fn set_git_command_override(v: Option<String>) {
    *GIT_COMMAND_OVERRIDE.lock().unwrap() = v;
}

pub fn get_git_command() -> Command {
    #[cfg(test)]
    {
        if let Some(p) = GIT_COMMAND_OVERRIDE.lock().unwrap().clone() {
            return Command::new(p);
        }
    }
    static GIT_PATH: OnceLock<String> = OnceLock::new();
    let path = GIT_PATH.get_or_init(|| {
        if Command::new("git")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return "git".to_string();
        }
        for p in [
            "/opt/homebrew/bin/git",
            "/usr/local/bin/git",
            "/usr/bin/git",
            "/bin/git",
        ] {
            if std::path::Path::new(p).exists() {
                return p.to_string();
            }
        }
        "git".to_string()
    });
    Command::new(path)
}
