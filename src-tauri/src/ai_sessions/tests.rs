use super::{
    build_native_terminal_applescript, clean_terminal_app_name, command_uses_resume_semantics,
    normalize_initial_prompt, normalize_terminal_app_key, normalize_working_dir_for_terminal,
    read_claude_project_file, read_codex_history_session_file, read_gemini_history_file,
    read_opencode_history_file, run_native_terminal_command_for_app_with_executor,
    select_gemini_session_for_create, select_gemini_session_for_existing, validate_create_command,
    GeminiSessionCandidate,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn make_temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "onespace-ai-sessions-{}-{}",
        name,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn write_temp_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, content).expect("write temp file");
}

#[test]
fn create_command_rejects_resume_flags() {
    assert!(command_uses_resume_semantics("gemini", "gemini -r latest"));
    assert!(command_uses_resume_semantics(
        "claude",
        "claude --resume abc"
    ));
    assert!(command_uses_resume_semantics("codex", "codex resume 123"));
    assert!(command_uses_resume_semantics(
        "opencode",
        "opencode --session 123"
    ));
}

#[test]
fn create_command_allows_plain_create_invocation() {
    assert!(!command_uses_resume_semantics("gemini", "gemini"));
    assert!(!command_uses_resume_semantics(
        "codex",
        "codex --profile p1"
    ));
    assert!(validate_create_command("opencode", "opencode --profile dev").is_ok());
}

#[test]
fn normalize_working_dir_handles_relative_and_home() {
    let dot = normalize_working_dir_for_terminal("./");
    assert!(dot.starts_with('/'));
    let home = normalize_working_dir_for_terminal("~");
    assert!(home.starts_with('/'));
}

#[test]
fn terminal_app_name_normalization_handles_bundle_suffix() {
    assert_eq!(clean_terminal_app_name(" Ghostty.app "), "Ghostty");
    assert_eq!(normalize_terminal_app_key("GHOSTTY.app"), "ghostty");
}

#[test]
fn native_terminal_applescript_uses_ghostty_window_launch() {
    let script = build_native_terminal_applescript(
        "Ghostty",
        "/tmp/ghostty-project",
        "codex resume 123",
        None,
    );
    assert!(script.contains("new surface configuration"));
    assert!(script
        .contains("set initial working directory of launch_config to \"/tmp/ghostty-project\""));
    assert!(
        script.contains("set initial input of launch_config to \"codex resume 123\" & linefeed")
    );
    assert!(script.contains("new window with configuration launch_config"));
    assert!(!script.contains("do script"));
}

#[test]
fn native_terminal_applescript_keeps_do_script_for_terminal() {
    let script = build_native_terminal_applescript(
        "Terminal",
        "/tmp/default-project",
        "codex resume 123",
        None,
    );
    assert!(script.contains("do script \"codex resume 123\""));
    assert!(!script.contains("new surface configuration"));
}

#[test]
fn native_terminal_runner_builds_ghostty_script_from_shared_entry() {
    let mut captured = String::new();
    run_native_terminal_command_for_app_with_executor(
        "Ghostty",
        "/tmp/ghostty-runner",
        "codex resume 123",
        None,
        None,
        |script| {
            captured = script;
            Ok(())
        },
    )
    .expect("capture ghostty script");

    assert!(captured.contains("new window with configuration launch_config"));
    assert!(captured.contains(
        "set initial input of launch_config to \"cd '/tmp/ghostty-runner' && codex resume 123\" & linefeed"
    ));
}

#[test]
fn native_terminal_runner_builds_standard_terminal_script_from_shared_entry() {
    let mut captured = String::new();
    run_native_terminal_command_for_app_with_executor(
        "Terminal",
        "/tmp/terminal-runner",
        "codex resume 123",
        None,
        None,
        |script| {
            captured = script;
            Ok(())
        },
    )
    .expect("capture terminal script");

    assert!(captured.contains("do script \"cd '/tmp/terminal-runner' && codex resume 123\""));
    assert!(!captured.contains("new window with configuration launch_config"));
}

#[test]
fn initial_prompt_is_injected_into_terminal_tab_not_shell_suffix() {
    let script = build_native_terminal_applescript(
        "Terminal",
        "/tmp/default-project",
        "codex resume 'session-1'",
        Some("/ai-flow-plan-coding 20260609-plan"),
    );
    assert!(script.contains("do script \"codex resume 'session-1'\""));
    assert!(script.contains("delay 1"));
    assert!(script.contains(
        "do script \"/ai-flow-plan-coding 20260609-plan\" in selected tab of front window"
    ));
    assert!(!script.contains("printf '%s"));
}

#[test]
fn initial_prompt_ignores_blank_values() {
    assert_eq!(normalize_initial_prompt(Some("  ")), None);
    assert_eq!(normalize_initial_prompt(None), None);
    assert_eq!(
        normalize_initial_prompt(Some(" /ai-flow-plan-coding slug ")).as_deref(),
        Some("/ai-flow-plan-coding slug")
    );
}

#[test]
fn initial_prompt_is_part_of_ghostty_initial_input() {
    let script = build_native_terminal_applescript(
        "Ghostty",
        "/tmp/ghostty-project",
        "codex",
        Some("/ai-flow-plan-orchestrate --resume queue-a"),
    );
    assert!(script.contains("set initial input of launch_config to \"codex"));
    assert!(script.contains("/ai-flow-plan-orchestrate --resume queue-a\" & linefeed"));
}

#[test]
fn codex_history_parser_reads_title_model_and_working_dir() {
    let root = make_temp_dir("codex-history");
    let path = root.join("rollout-2026-03-03T09-19-17-session-1.jsonl");
    write_temp_file(
        &path,
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"session-1\",\"timestamp\":\"2026-03-03T01:19:17.343Z\",\"cwd\":\"/tmp/codex-project\"}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.4\"}}\n"
        ),
    );

    let mut titles = HashMap::new();
    titles.insert("session-1".to_string(), "Codex Thread".to_string());
    let mut updated = HashMap::new();
    updated.insert("session-1".to_string(), 1_709_429_000_000_i64);

    let parsed = read_codex_history_session_file(&path, &titles, &updated, 1_709_428_000_000_i64)
        .expect("codex history entry");
    assert_eq!(parsed.title, "Codex Thread");
    assert_eq!(parsed.model_name.as_deref(), Some("gpt-5.4"));
    assert_eq!(
        parsed.working_dir,
        normalize_working_dir_for_terminal("/tmp/codex-project")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn codex_history_parser_falls_back_to_first_user_message_when_thread_name_missing() {
    let root = make_temp_dir("codex-history-user-title");
    let path = root.join("rollout-2026-03-03T09-19-17-session-2.jsonl");
    write_temp_file(
        &path,
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"session-2\",\"timestamp\":\"2026-03-03T01:19:17.343Z\",\"cwd\":\"/tmp/codex-project\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"Name this project better\"}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.4\"}}\n"
        ),
    );

    let parsed = read_codex_history_session_file(
        &path,
        &HashMap::new(),
        &HashMap::new(),
        1_709_428_000_000_i64,
    )
    .expect("codex history entry");
    assert_eq!(parsed.title, "Name this project better");
    assert_eq!(parsed.tool_session_id, "session-2");

    let path_without_title = root.join("rollout-2026-03-03T09-19-17-session-3.jsonl");
    write_temp_file(
        &path_without_title,
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"session-3\",\"timestamp\":\"2026-03-03T01:19:17.343Z\",\"cwd\":\"/tmp/codex-project\"}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.4\"}}\n"
        ),
    );

    let parsed_without_title = read_codex_history_session_file(
        &path_without_title,
        &HashMap::new(),
        &HashMap::new(),
        1_709_428_000_000_i64,
    )
    .expect("codex history entry without title");
    assert_eq!(parsed_without_title.title, "session-3");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn claude_history_parser_prefers_last_prompt_and_reads_model() {
    let root = make_temp_dir("claude-history");
    let path = root.join("session-1.jsonl");
    write_temp_file(
        &path,
        concat!(
            "{\"type\":\"user\",\"sessionId\":\"session-1\",\"cwd\":\"/tmp/claude-project\",\"message\":{\"content\":\"first user prompt\"},\"timestamp\":\"2026-03-10T05:09:58.846Z\"}\n",
            "{\"type\":\"assistant\",\"sessionId\":\"session-1\",\"cwd\":\"/tmp/claude-project\",\"message\":{\"model\":\"qwen3.5-plus\"},\"timestamp\":\"2026-03-10T05:10:07.255Z\"}\n",
            "{\"type\":\"last-prompt\",\"sessionId\":\"session-1\",\"lastPrompt\":\"final prompt title\"}\n"
        ),
    );

    let parsed = read_claude_project_file(&path, None).expect("claude history entry");
    assert_eq!(parsed.title, "final prompt title");
    assert_eq!(parsed.model_name.as_deref(), Some("qwen3.5-plus"));
    assert_eq!(
        parsed.working_dir,
        normalize_working_dir_for_terminal("/tmp/claude-project")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn gemini_history_parser_reads_first_user_title_and_model() {
    let root = make_temp_dir("gemini-history");
    let path = root.join("session-gemini.json");
    write_temp_file(
        &path,
        r#"{
  "sessionId": "gemini-session-1",
  "projectHash": "project-1",
  "startTime": "2026-01-09T01:40:36.999Z",
  "lastUpdated": "2026-01-09T02:33:05.005Z",
  "messages": [
{ "type": "user", "content": "Gemini first prompt" },
{ "type": "gemini", "content": "Assistant reply", "model": "gemini-3-pro-preview" }
  ]
}"#,
    );
    let mut project_map = HashMap::new();
    project_map.insert(
        "project-1".to_string(),
        normalize_working_dir_for_terminal("/tmp/gemini-project"),
    );

    let parsed = read_gemini_history_file(&path, &project_map).expect("gemini history entry");
    assert_eq!(parsed.title, "Gemini first prompt");
    assert_eq!(parsed.model_name.as_deref(), Some("gemini-3-pro-preview"));
    assert_eq!(
        parsed.working_dir,
        normalize_working_dir_for_terminal("/tmp/gemini-project")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn opencode_history_parser_reads_title_and_message_model() {
    let root = make_temp_dir("opencode-history");
    let session_path = root.join("storage/session/project-1/session-1.json");
    let messages_root = root.join("storage/message");
    let project_worktree_by_id = HashMap::new();
    write_temp_file(
        &session_path,
        r#"{
  "id": "ses_123",
  "directory": "/tmp/opencode-project",
  "title": "OpenCode Session Title",
  "time": { "created": 1770800496647, "updated": 1770800790445 }
}"#,
    );
    write_temp_file(
        &messages_root.join("ses_123/msg_1.json"),
        r#"{
  "role": "assistant",
  "modelID": "gpt-5-codex"
}"#,
    );

    let parsed = read_opencode_history_file(&session_path, &messages_root, &project_worktree_by_id)
        .expect("opencode history");
    assert_eq!(parsed.title, "OpenCode Session Title");
    assert_eq!(parsed.model_name.as_deref(), Some("gpt-5-codex"));
    assert_eq!(
        parsed.working_dir,
        normalize_working_dir_for_terminal("/tmp/opencode-project")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn opencode_history_parser_falls_back_to_project_worktree_when_directory_missing() {
    let root = make_temp_dir("opencode-history-project-fallback");
    let session_path = root.join("storage/session/project-1/session-1.json");
    let messages_root = root.join("storage/message");
    let mut project_worktree_by_id = HashMap::new();
    project_worktree_by_id.insert(
        "project-1".to_string(),
        normalize_working_dir_for_terminal("/tmp/opencode-project-from-project"),
    );
    write_temp_file(
        &session_path,
        r#"{
  "id": "ses_456",
  "projectID": "project-1",
  "slug": "steady-signal",
  "time": { "created": 1770800496647, "updated": 1770800790445 }
}"#,
    );
    write_temp_file(
        &messages_root.join("ses_456/msg_1.json"),
        r#"{
  "role": "assistant",
  "modelID": "claude-opus-4-6"
}"#,
    );

    let parsed = read_opencode_history_file(&session_path, &messages_root, &project_worktree_by_id)
        .expect("opencode history with project fallback");
    assert_eq!(parsed.title, "steady-signal");
    assert_eq!(parsed.model_name.as_deref(), Some("claude-opus-4-6"));
    assert_eq!(
        parsed.working_dir,
        normalize_working_dir_for_terminal("/tmp/opencode-project-from-project")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn gemini_existing_binding_does_not_fallback_to_latest_when_created_time_present() {
    let created_at_ms = 1_700_000_000_000_i64;
    let candidates = vec![
        GeminiSessionCandidate {
            session_id: "older-but-updated".to_string(),
            start_at_ms: created_at_ms - 3_600_000,
            updated_at_ms: created_at_ms + 10_000,
        },
        GeminiSessionCandidate {
            session_id: "latest".to_string(),
            start_at_ms: created_at_ms - 7_200_000,
            updated_at_ms: created_at_ms + 20_000,
        },
    ];
    let selected = select_gemini_session_for_existing(&candidates, Some(created_at_ms));
    assert!(selected.is_none());
}

#[test]
fn gemini_existing_binding_prefers_start_time_over_recent_updates() {
    let created_at_ms = 1_700_000_000_000_i64;
    let candidates = vec![
        GeminiSessionCandidate {
            session_id: "target".to_string(),
            start_at_ms: created_at_ms + 2_000,
            updated_at_ms: created_at_ms + 15_000,
        },
        GeminiSessionCandidate {
            session_id: "distractor".to_string(),
            start_at_ms: created_at_ms - 7_200_000,
            updated_at_ms: created_at_ms + 30_000,
        },
    ];
    let selected = select_gemini_session_for_existing(&candidates, Some(created_at_ms));
    assert_eq!(selected.as_deref(), Some("target"));
}

#[test]
fn gemini_create_binding_prefers_nearest_start_time() {
    let launch_started_at_ms = 1_700_000_000_000_i64;
    let candidates = vec![
        GeminiSessionCandidate {
            session_id: "new".to_string(),
            start_at_ms: launch_started_at_ms + 1_000,
            updated_at_ms: launch_started_at_ms + 2_000,
        },
        GeminiSessionCandidate {
            session_id: "old-resumed".to_string(),
            start_at_ms: launch_started_at_ms - 3_600_000,
            updated_at_ms: launch_started_at_ms + 3_000,
        },
    ];
    let selected = select_gemini_session_for_create(&candidates, launch_started_at_ms);
    assert_eq!(selected.as_deref(), Some("new"));
}

#[test]
fn gemini_create_binding_falls_back_to_recent_update_when_no_near_start() {
    let launch_started_at_ms = 1_700_000_000_000_i64;
    let candidates = vec![
        GeminiSessionCandidate {
            session_id: "old-resumed".to_string(),
            start_at_ms: launch_started_at_ms - 86_400_000,
            updated_at_ms: launch_started_at_ms + 2_000,
        },
        GeminiSessionCandidate {
            session_id: "stale".to_string(),
            start_at_ms: launch_started_at_ms - 172_800_000,
            updated_at_ms: launch_started_at_ms - 1_000,
        },
    ];
    let selected = select_gemini_session_for_create(&candidates, launch_started_at_ms);
    assert_eq!(selected.as_deref(), Some("old-resumed"));
}

#[test]
#[ignore = "local environment smoke test"]
fn test_local_gemini_binding() {
    let working_dir = "/Users/yuqiyu/AiHistorys/one-space/onespace-app";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    use std::collections::HashSet;
    let exclude = HashSet::new();

    let candidates = super::collect_gemini_session_candidates(working_dir, Some(&exclude));
    println!("Found {} candidates for {}", candidates.len(), working_dir);
    for c in &candidates {
        println!(
            " - ID: {}, start: {}, updated: {}",
            c.session_id, c.start_at_ms, c.updated_at_ms
        );
    }

    let bind_time = now - 60000;
    let res = super::resolve_gemini_session_id_for_pending_bind(
        working_dir,
        Some(bind_time),
        Some(&exclude),
    );
    println!("Selected for pending bind (1m ago): {:?}", res);
}

#[test]
#[ignore = "local environment smoke test"]
fn test_local_claude_binding() {
    let working_dir = "/Users/yuqiyu/AiHistorys/one-space/onespace-app";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let res = super::resolve_claude_session_id(working_dir, now, None);
    println!("Resolved claude session (now): {:?}", res);

    // 使用实际的历史记录时间戳测试
    let test_timestamp = 1773388541848_i64; // 最近一条记录的时间
    let res_past =
        super::resolve_claude_session_id("/Users/yuqiyu/AiHistorys", test_timestamp, None);
    println!("Resolved claude session (historical): {:?}", res_past);
    assert!(res_past.is_some(), "Should find historical session");
}

// --- Permission mode command building tests ---

#[test]
fn build_resume_command_default_keeps_existing_behavior() {
    // Default permission mode should produce the same command as the non-permission variant
    let claude =
        super::build_resume_command("claude", "sess1", super::TerminalPermissionMode::Default);
    assert!(claude.is_some());
    let claude_cmd = &claude.unwrap().command;
    assert!(claude_cmd.starts_with("claude -r "));
    assert!(!claude_cmd.contains("--dangerously-skip-permissions"));

    let gemini =
        super::build_resume_command("gemini", "sess2", super::TerminalPermissionMode::Default);
    assert!(gemini.is_some());
    let gemini_cmd = &gemini.unwrap().command;
    assert!(gemini_cmd.starts_with("gemini -r "));
    assert!(!gemini_cmd.contains("--approval-mode=yolo"));

    let codex =
        super::build_resume_command("codex", "sess3", super::TerminalPermissionMode::Default);
    assert!(codex.is_some());
    let codex_cmd = &codex.unwrap().command;
    assert!(codex_cmd.starts_with("codex resume "));
    assert!(!codex_cmd.contains("--dangerously-bypass-approvals-and-sandbox"));

    let opencode =
        super::build_resume_command("opencode", "sess4", super::TerminalPermissionMode::Default);
    assert!(opencode.is_some());
    let opencode_result = opencode.unwrap();
    assert_eq!(opencode_result.command, "opencode -s 'sess4'");
    assert!(opencode_result.env.is_none());
}

#[test]
fn build_resume_command_full_access_claude() {
    let result = super::build_resume_command(
        "claude",
        "abc123",
        super::TerminalPermissionMode::FullAccess,
    );
    assert!(result.is_some());
    let r = result.unwrap();
    assert!(r.command.contains("--dangerously-skip-permissions"));
    assert!(r.command.contains("-r 'abc123'"));
    assert!(r.env.is_none());
}

#[test]
fn build_resume_command_full_access_gemini() {
    let result = super::build_resume_command(
        "gemini",
        "xyz789",
        super::TerminalPermissionMode::FullAccess,
    );
    assert!(result.is_some());
    let r = result.unwrap();
    assert!(r.command.contains("--approval-mode=yolo"));
    assert!(r.command.contains("-r 'xyz789'"));
    assert!(r.env.is_none());
}

#[test]
fn build_resume_command_full_access_codex() {
    let result = super::build_resume_command(
        "codex",
        "codex42",
        super::TerminalPermissionMode::FullAccess,
    );
    assert!(result.is_some());
    let r = result.unwrap();
    assert!(r
        .command
        .contains("--dangerously-bypass-approvals-and-sandbox"));
    assert!(r.command.contains("resume 'codex42'"));
    assert!(r.env.is_none());
}

#[test]
fn build_resume_command_full_access_opencode() {
    let result = super::build_resume_command(
        "opencode",
        "op55",
        super::TerminalPermissionMode::FullAccess,
    );
    assert!(result.is_some());
    let r = result.unwrap();
    assert_eq!(r.command, "opencode -s 'op55'");
    let env = r.env.expect("opencode full_access should set env");
    assert_eq!(env.get("OPENCODE_PERMISSION"), Some(&"allow".to_string()));
}

#[test]
fn build_resume_command_empty_session_id_returns_none() {
    assert!(
        super::build_resume_command("claude", "", super::TerminalPermissionMode::Default).is_none()
    );
    assert!(super::build_resume_command(
        "claude",
        "   ",
        super::TerminalPermissionMode::FullAccess
    )
    .is_none());
}

#[test]
fn build_resume_command_unknown_tool_returns_none() {
    assert!(
        super::build_resume_command("unknown", "s1", super::TerminalPermissionMode::Default)
            .is_none()
    );
}
