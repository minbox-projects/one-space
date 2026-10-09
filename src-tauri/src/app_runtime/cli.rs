use crate::app_store;
use std::fs::{self, File};
use std::io::Write;
use std::process::Command;
use std::sync::OnceLock;

#[allow(dead_code)]
pub(super) fn get_brew_command() -> Command {
    static BREW_PATH: OnceLock<String> = OnceLock::new();
    let path = BREW_PATH.get_or_init(|| {
        if Command::new("brew")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return "brew".to_string();
        }
        for p in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
            if std::path::Path::new(p).exists() {
                return p.to_string();
            }
        }
        "brew".to_string()
    });
    Command::new(path)
}

pub fn get_git_command() -> Command {
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

pub(super) const INTERNAL_CLI_RESOLVE_SESSION_COMMAND: &str = "__onespace_cli_resolve_session";
pub(super) const INTERNAL_CLI_CREATE_SESSION_COMMAND: &str = "__onespace_cli_create_session";
pub(super) const INTERNAL_CLI_RESUME_SESSION_COMMAND: &str = "__onespace_cli_resume_session";
pub(super) const INTERNAL_CLI_CLAUDE_PROFILE_SET_DEFAULT: &str =
    "__onespace_cli_claude_profile_set_default";
pub(super) const INTERNAL_CLI_GET_CLAUDE_CONFIG_DIR: &str = "__onespace_cli_get_claude_config_dir";
pub(super) const INTERNAL_CLI_LIST_CLAUDE_PROFILES: &str = "__onespace_cli_list_claude_profiles";
pub(super) const INTERNAL_CLI_ENV_LIST: &str = "__onespace_cli_env_list";
pub(super) const INTERNAL_CLI_ENV_USE: &str = "__onespace_cli_env_use";

pub(super) fn handle_internal_cli_command() -> bool {
    let mut args = std::env::args();
    let _ = args.next();
    let Some(command) = args.next() else {
        return false;
    };

    match command.as_str() {
        INTERNAL_CLI_RESUME_SESSION_COMMAND => {
            let args = args.collect::<Vec<_>>();
            match tauri::async_runtime::block_on(app_store::session_service::resume_cli_session(
                &args,
            )) {
                Ok((_, status)) => {
                    let code = status.code().unwrap_or_else(|| {
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            128 + status.signal().unwrap_or(1)
                        }
                        #[cfg(not(unix))]
                        {
                            1
                        }
                    });
                    std::process::exit(code);
                }
                Err(error) => {
                    eprintln!("{}: {}", error.code, error.message);
                    std::process::exit(1);
                }
            }
        }
        INTERNAL_CLI_CREATE_SESSION_COMMAND => {
            let args = args.collect::<Vec<_>>();
            match tauri::async_runtime::block_on(app_store::session_service::create_cli_session(
                &args,
            )) {
                Ok((_, status)) => {
                    let code = status.code().unwrap_or_else(|| {
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            128 + status.signal().unwrap_or(1)
                        }
                        #[cfg(not(unix))]
                        {
                            1
                        }
                    });
                    std::process::exit(code);
                }
                Err(error) => {
                    eprintln!("{}: {}", error.code, error.message);
                    std::process::exit(1);
                }
            }
        }
        INTERNAL_CLI_RESOLVE_SESSION_COMMAND => {
            let query = args.next().unwrap_or_default();
            match app_store::cli_lookup_session(&query) {
                Ok(Some(record)) => {
                    println!(
                        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                        record.tool, record.tool_session_id, record.working_dir, record.id
                    );
                    std::process::exit(0);
                }
                Ok(None) => {
                    eprintln!("Session not found: {}", query);
                    std::process::exit(1);
                }
                Err(err) => {
                    eprintln!("Failed to resolve session: {}", err);
                    std::process::exit(1);
                }
            }
        }
        INTERNAL_CLI_CLAUDE_PROFILE_SET_DEFAULT => {
            let query = args.next().unwrap_or_default();
            let _operation = match app_store::lock_service_provider_operation() {
                Ok(operation) => operation,
                Err(error) => {
                    eprintln!("Failed to lock providers: {}", error);
                    std::process::exit(1);
                }
            };
            let mut state = match app_store::load_service_providers_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to load providers: {}", e);
                    std::process::exit(1);
                }
            };
            let profile_id = match state.providers.iter().find(|provider| {
                provider.tool == "claude"
                    && (provider.id == query
                        || provider.name == query
                        || provider.code.as_deref() == Some(query.as_str()))
            }) {
                Some(p) => p.id.clone(),
                None => {
                    eprintln!("Claude profile not found: {query}");
                    std::process::exit(1);
                }
            };
            state.active.insert("claude".to_string(), profile_id);
            if let Err(e) = app_store::save_service_providers_internal(&state) {
                eprintln!("Failed to save providers: {}", e);
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        INTERNAL_CLI_GET_CLAUDE_CONFIG_DIR => {
            let query = args.next().unwrap_or_default();
            let state = match crate::app_store::load_service_providers_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to load providers: {}", e);
                    std::process::exit(1);
                }
            };
            let provider = state.providers.iter().find(|provider| {
                provider.tool == "claude"
                    && (provider.id == query
                        || provider.name == query
                        || provider.code.as_deref() == Some(query.as_str()))
            });
            match provider {
                Some(_) => {
                    let dir = match crate::app_store::resolve_claude_profile_config_dir(&query) {
                        Ok(dir) => dir,
                        Err(e) => {
                            eprintln!("{}", e);
                            std::process::exit(1);
                        }
                    };
                    println!("{}", dir.to_string_lossy());
                    std::process::exit(0);
                }
                None => {
                    eprintln!("Claude profile not found: {query}");
                    std::process::exit(1);
                }
            }
        }
        INTERNAL_CLI_LIST_CLAUDE_PROFILES => {
            let state = match crate::app_store::load_service_providers_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to load providers: {}", e);
                    std::process::exit(1);
                }
            };
            println!("Claude Profiles:");
            println!("----------------");
            for provider in state
                .providers
                .iter()
                .filter(|provider| provider.tool == "claude")
            {
                let default_mark = if state.active.get("claude") == Some(&provider.id) {
                    " [default]"
                } else {
                    ""
                };
                let profile_ref = provider.code.as_deref().unwrap_or(provider.id.as_str());
                let config_dir = crate::claude_profiles::get_claude_profiles_dir()
                    .map(|dir| dir.join(profile_ref))
                    .map(|dir| dir.to_string_lossy().to_string())
                    .unwrap_or_else(|_| String::new());
                println!("  {} ({}){}", provider.name, profile_ref, default_mark);
                if provider.code.is_some() {
                    println!("    Code: {}", profile_ref);
                }
                println!("    Config Dir: {}", config_dir);
            }
            std::process::exit(0);
        }
        INTERNAL_CLI_ENV_LIST => {
            match app_store::provider_activation::cli_environment_listing() {
                Ok(listing) => println!("{}", listing),
                Err(error) => {
                    eprintln!("{}", error);
                    std::process::exit(1);
                }
            }
            std::process::exit(0);
        }
        INTERNAL_CLI_ENV_USE => {
            let tool = args.next().unwrap_or_default();
            let target = args.next().unwrap_or_default();
            let provider_id = match app_store::provider_activation::use_cli_environment(&tool, &target) {
                Ok(provider_id) => provider_id,
                Err(error) => {
                    eprintln!("{}", error);
                    std::process::exit(1);
                }
            };
            println!(
                "Switched {} to environment: {} ({})",
                tool, target, provider_id
            );
            std::process::exit(0);
        }
        _ => return false,
    }
}

#[tauri::command]
pub(super) fn install_cli() -> Result<(), String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let local_bin = home_dir.join(".local").join("bin");
    if !local_bin.exists() {
        fs::create_dir_all(&local_bin).map_err(|e| e.to_string())?;
    }
    let script_path = local_bin.join("onespace");

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let app_bin = current_exe.to_string_lossy().to_string();

    let mut file = File::create(&script_path).map_err(|e| e.to_string())?;

    let script_content = build_cli_script_content("", &app_bin);

    file.write_all(script_content.as_bytes())
        .map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn build_cli_script_content(_sessions_file: &str, app_bin: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash

# OneSpace AI CLI Tool
# Usage:
#   onespace ai <model_shortcut> [session_name] [--permission-mode default|full_access]
#   onespace resume <session_id> [--permission-mode default|full_access]
#   onespace env list
#   onespace env use <tool> <provider_name_or_id>

APP_BIN={}
CONFIG_FILE="$HOME/.config/onespace/config.json"

resolve_claude_command() {{
    if [ -f "$CONFIG_FILE" ]; then
        LAUNCH_CMD=$(python3 -c "
import json, sys
try:
    cfg = json.load(open(sys.argv[1]))
    cmds = cfg.get('ai_model_launch_commands', {{}})
    cmd = cmds.get('claude', '').strip()
    if cmd:
        print(cmd)
except:
    pass
" "$CONFIG_FILE" 2>/dev/null)
        if [ -n "$LAUNCH_CMD" ]; then
            echo "$LAUNCH_CMD"
            return
        fi
    fi
    echo "claude"
}}

print_help() (
    cat <<'EOF'
OneSpace CLI

Usage:
  onespace <command> [options]

Commands:
  ai <model_shortcut> [session_name] [extra args...] [--permission-mode default|full_access]
      Start an AI terminal session in current working directory.
      Models: claude, antigravity, opencode, codex
      A full_access-configured tool requires an explicit --permission-mode choice.

  resume <session_id> [--permission-mode default|full_access]
      Resume a saved session by Session ID from OneSpace AI Sessions.
      A tool configured for full_access requires an explicit permission choice.

  claude profile <subcommand>
      Manage Claude profiles (list, set, launch).

  env list
      List configured provider environments and active bindings.

  env use <tool> <provider_name_or_id>
      Switch active provider for a tool.

Options:
  -h, --help    Show this help message

Examples:
  onespace ai claude my_session
  onespace ai antigravity
  onespace ai claude my_session --permission-mode full_access
  onespace resume 9b6f4b6e-2c63-4a11-9f7a-demo
  onespace claude profile list
  onespace claude profile set work
  onespace claude profile work
  onespace env list
  onespace env use claude my-provider
EOF
)

print_claude_profile_help() (
    cat <<'EOF'
Usage:
  onespace claude profile list                           List Claude profiles
  onespace claude profile set <profile>                  Set default Claude profile
  onespace claude profile <profile> [-- <claude args>]   Launch Claude with profile

Examples:
  onespace claude profile list
  onespace claude profile set work
  onespace claude profile work -- --model opus
EOF
)

print_env_help() (
    cat <<'EOF'
Usage:
  onespace env list
  onespace env use <tool> <provider_name_or_id>
EOF
)

print_ai_help() (
    cat <<'EOF'
Usage:
  onespace ai <model_shortcut> [session_name] [extra args...] [--permission-mode default|full_access]

Models:
  claude, antigravity, opencode, codex

A tool configured for full_access requires an explicit permission choice:
  --permission-mode full_access   confirm elevation for this session
  --permission-mode default       run unprivileged
EOF
)

print_resume_help() (
    cat <<'EOF'
Usage:
  onespace resume <session_id> [--permission-mode default|full_access]

Resume a saved OneSpace session by Session ID copied from AI Sessions.
OneSpace prepares the native ID, working directory and provider/runtime environment.
Use --permission-mode full_access to confirm a configured full_access tool,
or --permission-mode default to resume without that elevation.
EOF
)

if [ -z "$1" ] || [ "$1" == "--help" ] || [ "$1" == "-h" ]; then
    print_help
    exit 0
fi

if [ "$1" == "resume" ]; then
    if [ -z "$2" ] || [ "$2" == "--help" ] || [ "$2" == "-h" ]; then
        print_resume_help
        exit 0
    fi

    if [ ! -x "$APP_BIN" ]; then
        echo "OneSpace app binary not found: $APP_BIN" >&2
        echo "Tip: reopen OneSpace and click Update CLI to refresh the installed script." >&2
        exit 1
    fi
    shift
    exec "$APP_BIN" __onespace_cli_resume_session "$@"
fi

# --- Claude Profile Management ---
if [ "$1" == "claude" ]; then
    if [ -z "$2" ] || [ "$2" == "--help" ] || [ "$2" == "-h" ]; then
        cat <<'EOF'
Usage:
  onespace claude profile <subcommand>

Manage Claude profiles. Use "onespace claude profile --help" for details.
EOF
        exit 0
    fi

    if [ "$2" != "profile" ]; then
        echo "Unknown claude command: $2"
        echo "Usage: onespace claude profile <subcommand>"
        exit 1
    fi

    if [ -z "$3" ] || [ "$3" == "--help" ] || [ "$3" == "-h" ]; then
        print_claude_profile_help
        exit 0
    fi

    if [ "$3" == "list" ]; then
        "$APP_BIN" __onespace_cli_list_claude_profiles
        exit $?
    fi
    if [ "$3" == "set" ]; then
        if [ -z "$4" ]; then
            echo "Usage: onespace claude profile set <profile_id>"
            exit 1
        fi
        PROFILE_ID="$4"
        "$APP_BIN" __onespace_cli_claude_profile_set_default "$PROFILE_ID"
        STATUS=$?
        if [ $STATUS -eq 0 ]; then
            echo "Default Claude profile set to: $PROFILE_ID"
        fi
        exit $STATUS
    fi

    # onespace claude profile <profile> [-- <claude args>]
    PROFILE_ID="$3"
    shift 3

    # Resolve profile config dir
    CONFIG_DIR=$("$APP_BIN" __onespace_cli_get_claude_config_dir "$PROFILE_ID")
    STATUS=$?
    if [ $STATUS -ne 0 ] || [ -z "$CONFIG_DIR" ]; then
        if [ $STATUS -eq 0 ]; then
            echo "Claude profile not found: $PROFILE_ID" >&2
        fi
        exit 1
    fi

    echo "Starting Claude with profile: $PROFILE_ID"
    echo "Config dir: $CONFIG_DIR"

    CLAUDE_CMD=$(resolve_claude_command)
    # 去掉命令中的 session_id 占位符（profile 启动是一次性新会话）
    CLAUDE_CMD=$(echo "$CLAUDE_CMD" | sed 's/ *--session-id *{{session_id}}//g' | sed 's/ *{{session_id}} *//g')
    echo "Launch command: $CLAUDE_CMD"

    if [ $# -gt 0 ] && [ "$1" == "--" ]; then
        shift
    fi

    CLAUDE_CONFIG_DIR="$CONFIG_DIR" exec $CLAUDE_CMD "$@"
fi

# --- Environment Management ---
if [ "$1" == "env" ]; then
    if [ -z "$2" ] || [ "$2" == "--help" ] || [ "$2" == "-h" ]; then
        print_env_help
        exit 0
    fi

    if [ "$2" == "list" ]; then
        "$APP_BIN" __onespace_cli_env_list
        exit $?
    elif [ "$2" == "use" ]; then
        TOOL="$3"
        TARGET="$4"
        if [ -z "$TOOL" ] || [ -z "$TARGET" ]; then
            echo "Usage: onespace env use <tool> <provider_name_or_id>"
            exit 1
        fi

        "$APP_BIN" __onespace_cli_env_use "$TOOL" "$TARGET"
        exit $?
    else
        echo "Unknown env command: $2"
        print_env_help
        exit 1
    fi
fi

# --- AI Session Launcher ---
if [ "$1" != "ai" ]; then
    echo "Unknown command: $1"
    print_help
    exit 1
fi

if [ -z "$2" ] || [ "$2" == "--help" ] || [ "$2" == "-h" ]; then
    print_ai_help
    exit 0
fi

# Canonical registration and launch preparation belong to Rust. Preserve every
# argument and the invoking terminal instead of writing legacy Shell JSON.
shift
exec "$APP_BIN" __onespace_cli_create_session "$@"
"#,
        format!("'{}'", app_bin.replace('\'', "'\\''"))
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::build_cli_script_content;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn make_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "onespace-cli-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_executable(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create stub parent");
        }
        fs::write(path, content).expect("write stub");
        let mut perms = fs::metadata(path).expect("stub metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod stub");
    }

    fn find_plaintext_session_stores(root: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_plaintext_session_stores(&path, found);
            } else if path.file_name().and_then(|name| name.to_str()) == Some("ai_sessions.json") {
                found.push(path);
            }
        }
    }

    /// AC-001 canonical named CLI creation: the installed `onespace ai <name>`
    /// entry point must register the session through the Rust app binary
    /// (canonical writer) and must never store the supplied display name as the
    /// native session ID.
    ///
    /// The test runs the real generated CLI artifact in an isolated HOME with a
    /// stub app binary and a stub native tool, so no real user tool, credential
    /// or HOME history is touched.
    #[test]
    fn cli_ai_creation_delegates_canonical_registration_and_keeps_display_name() {
        let root = make_temp_dir("ai-create");
        let home = root.join("home");
        let workdir = root.join("project");
        let bin = root.join("bin");
        for dir in [&home, &workdir, &bin] {
            fs::create_dir_all(dir).expect("create fixture dir");
        }

        // The generated script writes its store at
        // `$HOME/.config/onespace/local_data/ai_sessions.json`; create the
        // parent so the RED isolates the intended behavior, not a missing dir.
        fs::create_dir_all(home.join(".config").join("onespace").join("local_data"))
            .expect("create isolated app dir");

        let app_log = root.join("app-bin.log");
        let native_log = root.join("native-tool.log");
        let native_cwd = root.join("native-cwd.log");

        // Stub app binary: records every invocation so the test can observe
        // whether the generated CLI delegates canonical registration to Rust.
        let app_bin = bin.join("onespace-app");
        write_executable(
            &app_bin,
            &format!(
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$@\" >> '{log}'\nexit 0\n",
                log = app_log.display()
            ),
        );

        // Stub the native tool so the generated script's `eval` can never reach
        // a real user installation.
        let native_tool = bin.join("claude");
        write_executable(
            &native_tool,
            &format!(
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$@\" > '{log}'\npwd > '{cwd}'\nexit 0\n",
                log = native_log.display(),
                cwd = native_cwd.display()
            ),
        );

        let script_path = bin.join("onespace");
        let sessions_file = home
            .join(".config")
            .join("onespace")
            .join("local_data")
            .join("ai_sessions.json");
        let script = build_cli_script_content(
            &sessions_file.to_string_lossy(),
            &app_bin.to_string_lossy(),
        );
        write_executable(&script_path, &script);

        let output = Command::new("bash")
            .arg(&script_path)
            .arg("ai")
            .arg("claude")
            .arg("my_session_name")
            .current_dir(&workdir)
            .env("HOME", &home)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .expect("run generated onespace CLI");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Independent expectation 1: canonical registration is delegated to the
        // app binary instead of a legacy Shell JSON write.
        let app_invocations = fs::read_to_string(&app_log).unwrap_or_default();
        assert!(
            !app_invocations.trim().is_empty(),
            "onespace ai must register the session through the app binary; stdout={stdout:?} stderr={stderr:?}"
        );
        assert!(
            app_invocations.contains("my_session_name"),
            "canonical registration must receive the supplied display name; argv={app_invocations:?}"
        );

        // Independent expectation 2: the supplied display name must never be
        // stored as the native session ID in any plaintext store the CLI writes.
        let mut stores = Vec::new();
        find_plaintext_session_stores(&home, &mut stores);
        for store in stores {
            let raw = fs::read_to_string(&store).expect("read session store");
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            let Some(entries) = parsed.as_array() else {
                continue;
            };
            for entry in entries {
                let name = entry
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let tool_session_id = entry
                    .get("tool_session_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                assert!(
                    tool_session_id.trim().is_empty() || tool_session_id != name,
                    "display name must not be stored as native session id in {}: {entry}",
                    store.display()
                );
            }
        }

        let _ = fs::remove_dir_all(&root);
    }

    /// AC-001 resume behavior through the real public CLI. Requires an explicit
    /// built app binary artifact (`ONESPACE_TEST_APP_BIN`); without it the smoke
    /// is truthfully skipped so ordinary runs never depend on a stale binary.
    ///
    /// Fixture: a dev-build app binary resolves `$HOME/.config/onespace-dev`, so
    /// the fixture symlinks that dev app dir to the release app dir the cfg(test)
    /// process writes, keeping one canonical store. The native tool is an
    /// isolated stub; only public services seed state.
    #[test]
    #[ignore]
    fn resume_cli_uses_canonical_bound_id_and_provider_profile_environment() {
        use crate::app_store::{
            session_service, ServiceProviderRecord, ServiceProvidersState, SessionInput,
        };
        use std::os::unix::fs::symlink;

        let Some(app_bin) = std::env::var_os("ONESPACE_TEST_APP_BIN").map(PathBuf::from) else {
            eprintln!("skip resume smoke: ONESPACE_TEST_APP_BIN is not set");
            return;
        };
        if !app_bin.is_file() {
            eprintln!(
                "skip resume smoke: ONESPACE_TEST_APP_BIN is not a file: {}",
                app_bin.display()
            );
            return;
        }

        let _guard = crate::app_runtime::lock_test_home_env();
        let root = make_temp_dir("resume-smoke");
        let home = root.join("home");
        let workdir = root.join("project");
        let stub_bin = root.join("bin");
        for dir in [&home, &workdir, &stub_bin] {
            fs::create_dir_all(dir).expect("create fixture dir");
        }
        let release_app = home.join(".config").join("onespace");
        let dev_app = home.join(".config").join("onespace-dev");
        fs::create_dir_all(&release_app).expect("create release app dir");
        symlink(&release_app, &dev_app).expect("link dev app dir to release app dir");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let provider_id = uuid::Uuid::new_v4().to_string();
            let mut providers = ServiceProvidersState::default();
            providers.providers = vec![ServiceProviderRecord {
                id: provider_id.clone(),
                name: "Resume Provider".to_string(),
                tool: "claude".to_string(),
                api_key: "fixture-key".to_string(),
                ..ServiceProviderRecord::default()
            }];
            crate::app_store::save_service_providers_internal(&providers).expect("seed providers");

            let bound_input = SessionInput {
                id: None,
                name: "resume display name".to_string(),
                working_dir: workdir.to_string_lossy().to_string(),
                tool: "claude".to_string(),
                tool_session_id: None,
                runtime_mode: None,
                runtime_profile_id: None,
                preset_id: None,
                status: None,
                provider_id: Some(provider_id.clone()),
                initial_prompt: None,
                permission_mode: None,
            };
            let (bound, expected_config_dir) = tauri::async_runtime::block_on(
                session_service::create_session(
                    bound_input,
                    |_record, _requested, _permission, options| {
                        let dir = options
                            .env
                            .as_ref()
                            .and_then(|env| env.get("CLAUDE_CONFIG_DIR"))
                            .cloned();
                        Ok((Some("native-abc-123".to_string()), dir))
                    },
                ),
            )
            .expect("create bound canonical session");
            let bound = bound.data;
            assert_eq!(bound.status, "active");
            assert_eq!(bound.tool_session_id, "native-abc-123");
            assert_eq!(bound.name, "resume display name");
            let expected_config_dir =
                expected_config_dir.expect("claude provider must yield CLAUDE_CONFIG_DIR");

            let argv_log = root.join("native-argv.log");
            let env_log = root.join("native-env.log");
            let cwd_log = root.join("native-cwd.log");
            write_executable(
                &stub_bin.join("claude"),
                &format!(
                    "#!/usr/bin/env bash\nprintf '%s\\n' \"$@\" > '{argv}'\nenv > '{env}'\npwd > '{cwd}'\nexit 0\n",
                    argv = argv_log.display(),
                    env = env_log.display(),
                    cwd = cwd_log.display()
                ),
            );

            let script_path = root.join("onespace");
            write_executable(
                &script_path,
                &build_cli_script_content("", &app_bin.to_string_lossy()),
            );

            let output = Command::new("bash")
                .arg(&script_path)
                .arg("resume")
                .arg(&bound.id)
                .env_clear()
                .env("HOME", &home)
                .env("PATH", format!("{}:/usr/bin:/bin", stub_bin.display()))
                .output()
                .expect("run resume");
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            assert!(output.status.success(), "resume must succeed: {stderr}");

            let argv = fs::read_to_string(&argv_log).expect("native argv captured");
            assert!(argv.contains("-r"), "resume must pass the native flag: {argv:?}");
            assert!(
                argv.contains("native-abc-123"),
                "resume must pass the real native ID: {argv:?}"
            );
            assert!(
                !argv.contains("resume display name"),
                "display name must never be used as the native ID: {argv:?}"
            );
            let cwd = fs::read_to_string(&cwd_log).expect("native cwd captured");
            let cwd_canonical = fs::canonicalize(cwd.trim())
                .unwrap_or_else(|e| panic!("resume cwd is not an existing path: {cwd:?}: {e}"));
            assert_eq!(
                cwd_canonical,
                fs::canonicalize(&workdir).expect("canonicalize fixture workdir"),
                "resume must run in the original working directory"
            );
            let env = fs::read_to_string(&env_log).expect("native env captured");
            let actual_config_dir = env
                .lines()
                .find_map(|line| line.strip_prefix("CLAUDE_CONFIG_DIR="))
                .unwrap_or_else(|| {
                    panic!("resume must set CLAUDE_CONFIG_DIR for the selected provider")
                });
            let actual_canonical = fs::canonicalize(actual_config_dir).unwrap_or_else(|e| {
                panic!("resume CLAUDE_CONFIG_DIR is not an existing path: {actual_config_dir:?}: {e}")
            });
            let expected_canonical = fs::canonicalize(&expected_config_dir).unwrap_or_else(|e| {
                panic!("expected provider profile dir is not an existing path: {expected_config_dir:?}: {e}")
            });
            assert_eq!(
                actual_canonical, expected_canonical,
                "resume must use the same provider profile directory as create (canonical identity, not spelling)"
            );

            // Pending session with no real native ID must refuse and not launch.
            let pending_input = SessionInput {
                id: None,
                name: "pending display name".to_string(),
                working_dir: workdir.to_string_lossy().to_string(),
                tool: "claude".to_string(),
                tool_session_id: None,
                runtime_mode: None,
                runtime_profile_id: None,
                preset_id: None,
                status: None,
                provider_id: None,
                initial_prompt: None,
                permission_mode: None,
            };
            let (pending, _) = tauri::async_runtime::block_on(session_service::create_session(
                pending_input,
                |_record, _requested, _permission, _options| Ok((None, ())),
            ))
            .expect("create pending canonical session");
            assert_eq!(pending.data.status, "pending_bind");
            let _ = fs::remove_file(&argv_log);
            let pending_out = Command::new("bash")
                .arg(&script_path)
                .arg("resume")
                .arg(&pending.data.id)
                .env_clear()
                .env("HOME", &home)
                .env("PATH", format!("{}:/usr/bin:/bin", stub_bin.display()))
                .output()
                .expect("run pending resume");
            assert!(
                !pending_out.status.success(),
                "pending session resume must refuse"
            );
            assert!(
                !argv_log.exists(),
                "pending session must not launch the native tool"
            );
        }));

        match original_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        let _ = fs::remove_dir_all(&root);
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }

    /// AC-001 cohesive public CLI lifecycle through the real debug binary:
    /// `onespace ai` writes only the dev profile, discovers a real Claude native
    /// ID from a native history fixture, and `onespace resume <native id>`
    /// launches the native tool with that discovered ID and canonical cwd,
    /// leaving an independently seeded release-profile sentinel byte-identical.
    ///
    /// Requires the built app binary (`ONESPACE_TEST_APP_BIN`); without it the
    /// smoke skips truthfully. No process HOME mutation, no app-store seeding and
    /// no profile symlink: the spawned debug binary naturally resolves the dev
    /// app dir while the release sentinel is seeded independently on disk.
    #[test]
    #[ignore]
    fn cli_creation_uses_debug_profile_binds_real_claude_history_and_leaves_release_sentinel() {
        struct RemoveDir(PathBuf);
        impl Drop for RemoveDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        const NATIVE_ID: &str = "native-claude-777";
        const DISPLAY_NAME: &str = "lifecycle display name";
        const SENTINEL: &[u8] = b"release-sentinel-must-not-change";

        let Some(app_bin) = std::env::var_os("ONESPACE_TEST_APP_BIN").map(PathBuf::from) else {
            eprintln!("skip creation lifecycle smoke: ONESPACE_TEST_APP_BIN is not set");
            return;
        };
        if !app_bin.is_file() {
            eprintln!(
                "skip creation lifecycle smoke: ONESPACE_TEST_APP_BIN is not a file: {}",
                app_bin.display()
            );
            return;
        }

        let root = make_temp_dir("creation-lifecycle");
        let _cleanup = RemoveDir(root.clone());
        let home = root.join("home");
        let workdir = root.join("project");
        let stub_bin = root.join("bin");
        for dir in [&home, &workdir, &stub_bin] {
            fs::create_dir_all(dir).expect("create fixture dir");
        }

        // Independently seed the release profile sentinel bytes; the debug
        // binary must never write the release profile.
        let release_sentinel = home
            .join(".config")
            .join("onespace")
            .join("data")
            .join("data")
            .join("sessions")
            .join("state.json");
        fs::create_dir_all(release_sentinel.parent().expect("sentinel parent"))
            .expect("create release parent");
        fs::write(&release_sentinel, SENTINEL).expect("seed release sentinel");

        // Native stub: writes a real Claude history line to the resolver's
        // default path when no provider env is set, then logs argv/cwd.
        let argv_log = root.join("native-argv.log");
        let cwd_log = root.join("native-cwd.log");
        write_executable(
            &stub_bin.join("claude"),
            &format!(
                "#!/usr/bin/env bash\nmkdir -p \"$HOME/.claude\"\nTS=$(($(date +%s) * 1000))\nprintf '{{\"sessionId\":\"{native}\",\"timestamp\":%s,\"project\":\"%s\"}}\\n' \"$TS\" \"$(pwd -P)\" > \"$HOME/.claude/history.jsonl\"\nprintf '%s\\n' \"$@\" >> '{argv}'\npwd -P >> '{cwd}'\nexit 0\n",
                native = NATIVE_ID,
                argv = argv_log.display(),
                cwd = cwd_log.display(),
            ),
        );

        let script_path = root.join("onespace");
        write_executable(
            &script_path,
            &build_cli_script_content("", &app_bin.to_string_lossy()),
        );
        let child_path = format!("{}:/usr/bin:/bin", stub_bin.display());

        let create = Command::new("bash")
            .arg(&script_path)
            .arg("ai")
            .arg("claude")
            .arg(DISPLAY_NAME)
            .current_dir(&workdir)
            .env_clear()
            .env("HOME", &home)
            .env("PATH", &child_path)
            .output()
            .expect("run onespace ai");
        assert!(
            create.status.success(),
            "onespace ai must succeed: stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&create.stdout),
            String::from_utf8_lossy(&create.stderr)
        );

        // Release sentinel bytes unchanged; dev canonical state exists separately.
        assert_eq!(
            fs::read(&release_sentinel).expect("read release sentinel"),
            SENTINEL,
            "debug CLI must not touch the release profile"
        );
        let dev_sessions = home
            .join(".config")
            .join("onespace-dev")
            .join("data")
            .join("data")
            .join("sessions")
            .join("state.json");
        assert!(
            dev_sessions.is_file(),
            "debug profile must hold the canonical session: {}",
            dev_sessions.display()
        );

        // Resume by the discovered native ID must launch that ID in the cwd.
        let resume = Command::new("bash")
            .arg(&script_path)
            .arg("resume")
            .arg(NATIVE_ID)
            .current_dir(&workdir)
            .env_clear()
            .env("HOME", &home)
            .env("PATH", &child_path)
            .output()
            .expect("run onespace resume");
        assert!(
            resume.status.success(),
            "onespace resume must succeed: stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&resume.stdout),
            String::from_utf8_lossy(&resume.stderr)
        );
        let argv = fs::read_to_string(&argv_log).expect("native argv");
        assert!(
            argv.lines().any(|line| line == "-r"),
            "resume must use the native flag: {argv:?}"
        );
        assert!(
            argv.lines().any(|line| line == NATIVE_ID),
            "resume must use the discovered native ID: {argv:?}"
        );
        assert!(
            !argv.contains(DISPLAY_NAME),
            "display name must not be used as the native ID: {argv:?}"
        );
        let cwd = fs::read_to_string(&cwd_log).expect("native cwd");
        let expected_cwd = fs::canonicalize(&workdir).expect("canonicalize fixture workdir");
        assert!(
            cwd.lines()
                .any(|line| fs::canonicalize(line).map(|p| p == expected_cwd).unwrap_or(false)),
            "resume must run in the canonical working directory: {cwd:?}"
        );
    }

    /// AC-001 CLI create permission confirmation: with a tool configured
    /// `full_access`, `onespace ai` without an explicit choice must refuse and
    /// not launch; an explicit `--permission-mode default` must run unprivileged
    /// and `full_access` must carry the elevation flag. The dev profile config is
    /// written directly as a JSON file fixture (no cfg(test) config writer).
    #[test]
    #[ignore]
    fn cli_creation_permission_mode_confirmation_controls_elevation() {
        struct RemoveDir(PathBuf);
        impl Drop for RemoveDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        let Some(app_bin) = std::env::var_os("ONESPACE_TEST_APP_BIN").map(PathBuf::from) else {
            eprintln!("skip creation permission smoke: ONESPACE_TEST_APP_BIN is not set");
            return;
        };
        if !app_bin.is_file() {
            eprintln!(
                "skip creation permission smoke: ONESPACE_TEST_APP_BIN is not a file: {}",
                app_bin.display()
            );
            return;
        }

        let root = make_temp_dir("creation-permission");
        let _cleanup = RemoveDir(root.clone());
        let home = root.join("home");
        let workdir = root.join("project");
        let stub_bin = root.join("bin");
        for dir in [&home, &workdir, &stub_bin] {
            fs::create_dir_all(dir).expect("create fixture dir");
        }
        let dev_config = home
            .join(".config")
            .join("onespace-dev")
            .join("config.json");
        fs::create_dir_all(dev_config.parent().expect("dev config parent"))
            .expect("create dev config parent");
        let dev_sessions = home
            .join(".config")
            .join("onespace-dev")
            .join("data")
            .join("data")
            .join("sessions")
            .join("state.json");
        let argv_log = root.join("native-argv.log");
        write_executable(
            &stub_bin.join("claude"),
            &format!(
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$@\" >> '{log}'\nexit 0\n",
                log = argv_log.display()
            ),
        );
        let script_path = root.join("onespace");
        write_executable(
            &script_path,
            &build_cli_script_content("", &app_bin.to_string_lossy()),
        );
        let child_path = format!("{}:/usr/bin:/bin", stub_bin.display());

        let run_create = |extra: &[&str]| -> std::process::Output {
            let mut command = Command::new("bash");
            command.arg(&script_path).arg("ai").arg("claude");
            for arg in extra {
                command.arg(arg);
            }
            command
                .current_dir(&workdir)
                .env_clear()
                .env("HOME", &home)
                .env("PATH", &child_path)
                .output()
                .expect("run onespace ai")
        };

        // Tool configured full_access in the dev profile.
        fs::write(
            &dev_config,
            "{\"storage_type\":\"local\",\"ai_model_permission_modes\":{\"claude\":\"full_access\"}}",
        )
        .expect("seed full_access config");

        // Warm-up run: first-run migration must not masquerade as a state change.
        let _ = run_create(&["warmup"]);
        let _ = fs::remove_file(&argv_log);

        // Phase A: no explicit confirmation must refuse without launching.
        let before = fs::read(&dev_sessions).ok();
        let phase_a = run_create(&["sample_one"]);
        let phase_a_err = String::from_utf8_lossy(&phase_a.stderr).into_owned();
        assert!(
            !phase_a.status.success(),
            "phase A must refuse without confirmation: {phase_a_err}"
        );
        assert!(
            phase_a_err.contains("PERMISSION_CONFIRMATION_REQUIRED"),
            "phase A stderr: {phase_a_err:?}"
        );
        assert!(
            !argv_log.exists(),
            "phase A must not launch the native tool"
        );
        assert_eq!(
            fs::read(&dev_sessions).ok(),
            before,
            "phase A must not change canonical sessions state"
        );

        // Phase B: explicit --permission-mode default must run unprivileged.
        // EXPECTED RED today: create_cli_session hardcodes permission_mode None
        // and misparses `--permission-mode` as the display name.
        let phase_b = run_create(&["--permission-mode", "default"]);
        let phase_b_out = String::from_utf8_lossy(&phase_b.stdout).into_owned();
        let phase_b_err = String::from_utf8_lossy(&phase_b.stderr).into_owned();
        assert!(
            phase_b.status.success(),
            "phase B explicit --permission-mode default must succeed: stdout={phase_b_out:?} stderr={phase_b_err:?}"
        );
        let argv_b = fs::read_to_string(&argv_log).unwrap_or_default();
        assert!(
            !argv_b.contains("default"),
            "phase B must not leak `default` into native argv: {argv_b:?}"
        );
        assert!(
            !argv_b.contains("--permission-mode"),
            "phase B must not leak the flag into native argv: {argv_b:?}"
        );
        assert!(
            !argv_b.contains("--dangerously-skip-permissions"),
            "phase B default must stay unprivileged: {argv_b:?}"
        );

        // Phase C: explicit full_access must carry the elevation flag.
        let _ = fs::remove_file(&argv_log);
        let phase_c = run_create(&["--permission-mode", "full_access"]);
        let phase_c_err = String::from_utf8_lossy(&phase_c.stderr).into_owned();
        assert!(
            phase_c.status.success(),
            "phase C full_access must succeed: stderr={phase_c_err:?}"
        );
        let argv_c = fs::read_to_string(&argv_log).unwrap_or_default();
        assert!(
            argv_c.contains("--dangerously-skip-permissions"),
            "phase C must elevate: {argv_c:?}"
        );
        assert!(
            !argv_c.contains("full_access"),
            "phase C must not leak the mode token: {argv_c:?}"
        );

        // Default-configured regression: unprivileged, no elevation flag.
        let _ = fs::remove_file(&argv_log);
        fs::write(
            &dev_config,
            "{\"storage_type\":\"local\",\"ai_model_permission_modes\":{\"claude\":\"default\"}}",
        )
        .expect("seed default config");
        let plain = run_create(&["sample_plain"]);
        let plain_err = String::from_utf8_lossy(&plain.stderr).into_owned();
        assert!(
            plain.status.success(),
            "default-configured create must succeed: stderr={plain_err:?}"
        );
        let argv_plain = fs::read_to_string(&argv_log).unwrap_or_default();
        assert!(
            !argv_plain.contains("--dangerously-skip-permissions"),
            "default config must stay unprivileged: {argv_plain:?}"
        );
    }
}
