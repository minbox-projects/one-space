use std::fs::{self, File};
use std::io::Write;

#[tauri::command]
pub(in crate::app_runtime) fn install_cli() -> Result<(), String> {
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

pub(in crate::app_runtime) fn build_cli_script_content(_sessions_file: &str, app_bin: &str) -> String {
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
