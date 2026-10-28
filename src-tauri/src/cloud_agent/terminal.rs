use std::process::Command;

pub fn escape_applescript_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn clean_terminal_app_name(app_name: &str) -> String {
    let trimmed = app_name.trim();
    if trimmed.to_lowercase().ends_with(".app") {
        trimmed[..trimmed.len() - 4].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn normalize_terminal_app_key(app_name: &str) -> String {
    clean_terminal_app_name(app_name).to_lowercase()
}

pub fn build_ssh_connect_command(
    host: &str,
    target: &str,
    branch: Option<&str>,
    use_yolo: bool,
    extra_args: &str,
) -> String {
    let mut remote_cmd = format!("cloud-session {}", target);
    if let Some(b) = branch.filter(|b| !b.is_empty()) {
        remote_cmd.push(' ');
        remote_cmd.push_str(b);
    }
    if !use_yolo {
        remote_cmd.push_str(" --no-yolo");
    }
    if !extra_args.trim().is_empty() {
        remote_cmd.push_str(" --extra-args ");
        remote_cmd.push_str(&format!("'{}'", extra_args.trim().replace('\'', "'\\''")));
    }

    format!("ssh -t {} \"{}\"", host, remote_cmd.replace('"', "\\\""))
}

pub fn launch_cloud_terminal_session(
    host: &str,
    target: &str,
    branch: Option<&str>,
    use_yolo: bool,
    extra_args: &str,
    terminal_override: Option<&str>,
) -> Result<(), String> {
    let configured_term = match terminal_override {
        Some(t) if !t.is_empty() && t != "auto" => t.to_string(),
        _ => crate::ai_sessions::resolve_terminal_app_name(),
    };
    let term_app_clean = clean_terminal_app_name(&configured_term);
    let term_key = normalize_terminal_app_key(&term_app_clean);
    let ssh_cmd = build_ssh_connect_command(host, target, branch, use_yolo, extra_args);

    // 针对不同终端进行分发
    if term_key == "ghostty" {
        let res = Command::new("open")
            .args(["-a", "Ghostty", "-n", "--args", "-e"])
            .arg("sh")
            .arg("-c")
            .arg(&ssh_cmd)
            .spawn();
        if res.is_ok() {
            return Ok(());
        }
    } else if term_key == "alacritty" {
        let res = Command::new("open")
            .args(["-a", "Alacritty", "-n", "--args", "-e"])
            .arg("sh")
            .arg("-c")
            .arg(&ssh_cmd)
            .spawn();
        if res.is_ok() {
            return Ok(());
        }
    } else if term_key == "iterm" || term_key == "iterm2" {
        let script = format!(
            r#"tell application "iTerm"
            activate
            try
                tell current window
                    create tab with default profile
                    tell current session
                        write text "{}"
                    end tell
                end tell
            on error
                create window with default profile
                tell current session of current window
                    write text "{}"
                end tell
            end try
        end tell"#,
            escape_applescript_string(&ssh_cmd),
            escape_applescript_string(&ssh_cmd)
        );
        let res = Command::new("osascript").arg("-e").arg(&script).spawn();
        if res.is_ok() {
            return Ok(());
        }
    }

    // 默认或失败回退使用 macOS Terminal.app
    let fallback_script = format!(
        r#"tell application "Terminal"
        activate
        do script "{}"
    end tell"#,
        escape_applescript_string(&ssh_cmd)
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&fallback_script)
        .spawn()
        .map_err(|e| format!("唤起本地终端失败: {}", e))?;

    Ok(())
}
