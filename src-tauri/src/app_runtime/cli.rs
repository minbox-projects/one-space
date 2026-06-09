use crate::{app_store, get_data_dir};
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
            let state = match crate::app_store::load_service_providers_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to load providers: {}", e);
                    std::process::exit(1);
                }
            };
            println!("Available Environments (Providers):");
            println!("----------------------------------");
            for provider in &state.providers {
                println!("{} -> {}", provider.tool, provider.name);
            }
            println!();
            println!("Current Active:");
            for (tool, provider_id) in &state.active {
                let name = state
                    .providers
                    .iter()
                    .find(|provider| provider.id == *provider_id)
                    .map(|provider| provider.name.as_str())
                    .unwrap_or(provider_id.as_str());
                println!("{} -> {}", tool, name);
            }
            std::process::exit(0);
        }
        INTERNAL_CLI_ENV_USE => {
            let tool = args.next().unwrap_or_default();
            let target = args.next().unwrap_or_default();
            if tool.trim().is_empty() || target.trim().is_empty() {
                eprintln!("Usage: onespace env use <tool> <provider_name_or_id>");
                std::process::exit(1);
            }
            let mut state = match crate::app_store::load_service_providers_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to load providers: {}", e);
                    std::process::exit(1);
                }
            };
            let provider_id = match state.providers.iter().find(|provider| {
                provider.tool == tool && (provider.id == target || provider.name == target)
            }) {
                Some(provider) => provider.id.clone(),
                None => {
                    eprintln!("Provider not found: {}", target);
                    std::process::exit(1);
                }
            };
            state.active.insert(tool.clone(), provider_id.clone());
            if let Err(e) = app_store::save_service_providers_internal(&state) {
                eprintln!("Failed to save providers: {}", e);
                std::process::exit(1);
            }
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

    let data_dir = get_data_dir()?;
    let sessions_path = data_dir.join("ai_sessions.json");
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let app_bin = current_exe.to_string_lossy().to_string();

    let mut file = File::create(&script_path).map_err(|e| e.to_string())?;

    let script_content = build_cli_script_content(&sessions_path.to_string_lossy(), &app_bin);

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

pub(super) fn build_cli_script_content(sessions_file: &str, app_bin: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash

# OneSpace AI CLI Tool
# Usage:
#   onespace ai <model_shortcut> [session_name]
#   onespace resume <session_id>
#   onespace env list
#   onespace env use <tool> <provider_name_or_id>

SESSIONS_FILE="{}"
APP_BIN="{}"
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

resolve_current_data_dir() (
    # v2 local-first storage layout
    local default_local="$HOME/.config/onespace/local_data"
    echo "$default_local"
)

DATA_DIR=$(resolve_current_data_dir)
if [ -n "$DATA_DIR" ] && [ "$DATA_DIR" != "." ]; then
    SESSIONS_FILE="$DATA_DIR/ai_sessions.json"
fi

print_help() (
    cat <<'EOF'
OneSpace CLI

Usage:
  onespace <command> [options]

Commands:
  ai <model_shortcut> [session_name] [extra args...]
      Start an AI terminal session in current working directory.
      Models: claude, gemini, opencode, codex

  resume <session_id>
      Resume a saved session by Session ID from OneSpace AI Sessions.

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
  onespace ai gemini
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
  onespace ai <model_shortcut> [session_name] [extra args...]
Models:
  claude, gemini, opencode, codex
EOF
)

print_resume_help() (
    cat <<'EOF'
Usage:
  onespace resume <session_id>

Resume a saved OneSpace session by Session ID copied from AI Sessions.
OneSpace will choose the correct native resume command for the tool.
EOF
)

resolve_session_record() (
    local lookup="$1"

    if [ -z "$lookup" ]; then
        echo "Usage: onespace resume <session_id>" >&2
        return 1
    fi

    if [ ! -x "$APP_BIN" ]; then
        echo "OneSpace app binary not found: $APP_BIN" >&2
        echo "Tip: reopen OneSpace and click Update CLI to refresh the installed script." >&2
        return 1
    fi

    "$APP_BIN" __onespace_cli_resolve_session "$lookup"
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

    SESSION_LOOKUP="$2"
    SESSION_RECORD=$(resolve_session_record "$SESSION_LOOKUP")
    STATUS=$?
    if [ $STATUS -ne 0 ]; then
        exit $STATUS
    fi

    IFS=$'\037' read -r SESSION_TOOL RESUME_TOOL_SESSION_ID SESSION_WORKING_DIR ONESPACE_SESSION_ID <<EOF
$SESSION_RECORD
EOF

    if [ -z "$RESUME_TOOL_SESSION_ID" ]; then
        echo "Session found, but native tool session ID is not available yet." >&2
        echo "Tip: wait for OneSpace history sync to finish, then retry." >&2
        exit 1
    fi

    case "$SESSION_TOOL" in
        claude)
            RESUME_CMD=(claude -r "$RESUME_TOOL_SESSION_ID")
            ;;
        gemini)
            RESUME_CMD=(gemini -r "$RESUME_TOOL_SESSION_ID")
            ;;
        opencode)
            RESUME_CMD=(opencode -s "$RESUME_TOOL_SESSION_ID")
            ;;
        codex)
            RESUME_CMD=(codex resume "$RESUME_TOOL_SESSION_ID")
            ;;
        *)
            echo "Unsupported session tool: $SESSION_TOOL" >&2
            exit 1
            ;;
    esac

    if [ -z "$SESSION_WORKING_DIR" ]; then
        echo "Session found, but working directory is missing." >&2
        echo "Refusing to resume outside the original session directory." >&2
        exit 1
    fi

    if [ ! -d "$SESSION_WORKING_DIR" ]; then
        echo "Original working directory not found: $SESSION_WORKING_DIR" >&2
        echo "Refusing to resume outside the original session directory." >&2
        exit 1
    fi

    cd "$SESSION_WORKING_DIR" || exit 1

    echo "Resuming OneSpace session: $SESSION_LOOKUP ($SESSION_TOOL)"
    exec "${{RESUME_CMD[@]}}"
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

MODEL_SHORTCUT="$2"
WORKING_DIR=$(pwd)
DIR_NAME=$(basename "$WORKING_DIR")

# Handle Session Name
if [ -n "$3" ]; then
    SESSION_NAME="$3"
else
    SESSION_NAME="${{DIR_NAME}}_ai"
fi

# Replace spaces and dots with underscores
SESSION_NAME=$(echo "$SESSION_NAME" | sed 's/[ .]/_/g')

# Map Models
case "$MODEL_SHORTCUT" in
    claude)
        CMD="claude code"
        TOOL_ID="claude"
        ;;
    gemini)
        CMD="gemini -y"
        TOOL_ID="gemini"
        ;;
    opencode)
        CMD="opencode"
        TOOL_ID="opencode"
        ;;
    codex)
        CMD="codex"
        TOOL_ID="codex"
        ;;
    *)
        echo "Unknown model: $MODEL_SHORTCUT"
        print_ai_help
        exit 1
        ;;
esac

# Sync to OneSpace (Write to ai_sessions.json)
CREATED_AT=$(date +%s)
SESSION_ID=$(uuidgen 2>/dev/null || echo "$CREATED_AT")
TOOL_SESSION_ID="$SESSION_NAME"

# Create simple JSON object
NEW_SESSION_JSON=$(printf '{{"id":"%s","name":"%s","working_dir":"%s","model_type":"%s","tool_session_id":"%s","created_at":%s}}' \
    "$SESSION_ID" "$SESSION_NAME" "$WORKING_DIR" "$TOOL_ID" "$TOOL_SESSION_ID" "$CREATED_AT")

# Add to JSON file if it exists, otherwise create new list
if [ -f "$SESSIONS_FILE" ]; then
    CONTENT=$(cat "$SESSIONS_FILE")
    if [[ "$CONTENT" == "[]" ]]; then
        echo "[$NEW_SESSION_JSON]" > "$SESSIONS_FILE"
    else
        echo "[${{NEW_SESSION_JSON}},${{CONTENT:1}}" > "$SESSIONS_FILE"
    fi
else
    echo "[$NEW_SESSION_JSON]" > "$SESSIONS_FILE"
fi

# Execute Command
if [ -n "$3" ]; then
    shift 3
else
    shift 2
fi

if [ $# -gt 0 ]; then
    CMD="$CMD $@"
fi

echo "Starting OneSpace AI session: $SESSION_NAME ($MODEL_SHORTCUT)"
eval "$CMD"
"#,
        sessions_file, app_bin
    )
}
