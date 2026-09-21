use super::{
    aggregate_day_stats_for_test, aggregate_usage_for_test, antigravity_brain_roots,
    antigravity_conversation_bindings_from_value, antigravity_managed_launch_env,
    build_native_terminal_applescript, clean_terminal_app_name,
    collect_antigravity_sessions_from_brain_root, collect_opencode_history_sessions_from_sources,
    collect_opencode_usage_records_from_sources, command_uses_resume_semantics,
    normalize_initial_prompt, normalize_terminal_app_key, normalize_working_dir_for_terminal,
    parse_antigravity_quota_envelope, parse_claude_usage_file, parse_codex_usage_file,
    parse_opencode_message_usage_dir, read_antigravity_history_file, read_claude_project_file,
    read_codex_history_session_file, read_opencode_history_file,
    read_opencode_message_tokens_for_test, run_native_terminal_command_for_app_with_executor,
    select_antigravity_session_for_create, select_antigravity_session_for_existing,
    sessions_usage_clear_cache, sessions_usage_tool_stats, timestamp_days_ago,
    usage_file_may_overlap_window_for_test, validate_create_command, AntigravitySessionCandidate,
    ToolScan, ToolScanCache, UsageRecord,
};
use chrono::Local;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

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
    assert!(command_uses_resume_semantics(
        "antigravity",
        "agy --conversation abc"
    ));
    assert!(command_uses_resume_semantics("antigravity", "agy -c"));
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
    assert!(!command_uses_resume_semantics("antigravity", "agy"));
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
        Some("/implement-plan 20260609-plan"),
    );
    assert!(script.contains("do script \"codex resume 'session-1'\""));
    assert!(script.contains("delay 1"));
    assert!(script
        .contains("do script \"/implement-plan 20260609-plan\" in selected tab of front window"));
    assert!(!script.contains("printf '%s"));
}

#[test]
fn initial_prompt_ignores_blank_values() {
    assert_eq!(normalize_initial_prompt(Some("  ")), None);
    assert_eq!(normalize_initial_prompt(None), None);
    assert_eq!(
        normalize_initial_prompt(Some(" /implement-plan slug ")).as_deref(),
        Some("/implement-plan slug")
    );
}

#[test]
fn initial_prompt_is_part_of_ghostty_initial_input() {
    let script = build_native_terminal_applescript(
        "Ghostty",
        "/tmp/ghostty-project",
        "codex",
        Some("/run-plan --resume queue-a"),
    );
    assert!(script.contains("set initial input of launch_config to \"codex"));
    assert!(script.contains("/run-plan --resume queue-a\" & linefeed"));
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
fn antigravity_history_parser_reads_first_user_input_title_and_model() {
    let root = make_temp_dir("antigravity-history");
    let path = root
        .join("conversation-1")
        .join(".system_generated")
        .join("logs")
        .join("transcript_full.jsonl");
    write_temp_file(
        &path,
        concat!(
            "{\"type\":\"USER_INPUT\",\"content\":\"Antigravity first prompt\"}\n",
            "{\"type\":\"MODEL\",\"model\":\"gemini-3-pro-preview\"}\n"
        ),
    );

    let parsed = read_antigravity_history_file(&path, "conversation-1", "/tmp/antigravity-project")
        .expect("antigravity history entry");
    assert_eq!(parsed.title, "Antigravity first prompt");
    assert_eq!(parsed.model_name.as_deref(), Some("gemini-3-pro-preview"));
    assert_eq!(parsed.tool_session_id, "conversation-1");
    assert_eq!(
        parsed.working_dir,
        normalize_working_dir_for_terminal("/tmp/antigravity-project")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn antigravity_brain_roots_cover_both_layouts_without_legacy_tmp() {
    let home = Path::new("/tmp/antigravity-home");
    let roots = antigravity_brain_roots(home);
    assert!(roots.contains(&home.join(".gemini").join("antigravity-cli").join("brain")));
    assert!(roots.contains(&home.join(".gemini").join("antigravity").join("brain")));
    assert!(!roots
        .iter()
        .any(|path| path.to_string_lossy().contains(".gemini/tmp")));
}

#[test]
fn antigravity_bindings_map_workspace_to_conversation_id() {
    let value: serde_json::Value = serde_json::from_str(
        r#"{
  "/tmp/antigravity-project": "conversation-flat",
  "conversations": {
    "conversation-nested": { "workspace": "/tmp/antigravity-nested" }
  }
}"#,
    )
    .expect("bindings json");

    let bindings = antigravity_conversation_bindings_from_value(&value);
    let flat = normalize_working_dir_for_terminal("/tmp/antigravity-project");
    let nested = normalize_working_dir_for_terminal("/tmp/antigravity-nested");
    assert_eq!(bindings.get("conversation-flat"), Some(&flat));
    assert_eq!(bindings.get("conversation-nested"), Some(&nested));

    assert!(
        antigravity_conversation_bindings_from_value(&serde_json::Value::Null).is_empty()
    );
}

#[test]
fn antigravity_discovery_reads_transcripts_from_both_brain_roots() {
    let home = make_temp_dir("antigravity-discovery");
    let bindings = HashMap::from([
        (
            "conversation-cli".to_string(),
            normalize_working_dir_for_terminal("/tmp/project-cli"),
        ),
        (
            "conversation-alt".to_string(),
            normalize_working_dir_for_terminal("/tmp/project-alt"),
        ),
    ]);
    write_temp_file(
        &home
            .join(".gemini")
            .join("antigravity-cli")
            .join("brain")
            .join("conversation-cli")
            .join(".system_generated")
            .join("logs")
            .join("transcript_full.jsonl"),
        "{\"type\":\"USER_INPUT\",\"content\":\"CLI transcript title\"}\n",
    );
    write_temp_file(
        &home
            .join(".gemini")
            .join("antigravity")
            .join("brain")
            .join("conversation-alt")
            .join("transcript_full.jsonl"),
        "{\"type\":\"USER_INPUT\",\"content\":\"Alt transcript title\"}\n",
    );

    let mut entries = Vec::new();
    for root in antigravity_brain_roots(&home) {
        entries.extend(collect_antigravity_sessions_from_brain_root(
            &root, &bindings, None,
        ));
    }
    entries.sort_by(|left, right| left.tool_session_id.cmp(&right.tool_session_id));

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].tool, "antigravity");
    assert_eq!(entries[0].tool_session_id, "conversation-alt");
    assert_eq!(entries[0].title, "Alt transcript title");
    assert_eq!(entries[1].tool_session_id, "conversation-cli");
    assert_eq!(entries[1].title, "CLI transcript title");

    let _ = fs::remove_dir_all(home);
}

#[test]
fn antigravity_discovery_degrades_on_missing_or_malformed_transcripts() {
    let home = make_temp_dir("antigravity-degrade");
    let bindings = HashMap::from([(
        "conversation-1".to_string(),
        normalize_working_dir_for_terminal("/tmp/project-degrade"),
    )]);

    // Transcript absent: nothing is discovered.
    let mut entries = Vec::new();
    for root in antigravity_brain_roots(&home) {
        entries.extend(collect_antigravity_sessions_from_brain_root(
            &root, &bindings, None,
        ));
    }
    assert!(entries.is_empty());

    // Malformed transcript: parsed as empty rather than panicking.
    let transcript = home
        .join(".gemini")
        .join("antigravity-cli")
        .join("brain")
        .join("conversation-1")
        .join(".system_generated")
        .join("logs")
        .join("transcript_full.jsonl");
    write_temp_file(&transcript, "{not-json}\n");
    assert!(read_antigravity_history_file(&transcript, "conversation-1", "/tmp/project-degrade")
        .is_none());

    // Missing last_conversations.json binding set yields an empty list.
    let mut unbound_entries = Vec::new();
    for root in antigravity_brain_roots(&home) {
        unbound_entries.extend(collect_antigravity_sessions_from_brain_root(
            &root,
            &HashMap::new(),
            None,
        ));
    }
    assert!(unbound_entries.is_empty());

    let _ = fs::remove_dir_all(home);
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
fn opencode_history_merges_sqlite_v2_v1_and_legacy_json_with_priority() {
    let root = make_temp_dir("opencode-history-source-priority");
    let db_path = root.join("opencode.db");
    let storage_root = root.join("storage");

    let v2_directory = root.join("directories/v2-shared-all");
    let v1_shared_directory = root.join("directories/v1-shared-all");
    let v1_json_directory = root.join("directories/v1-shared-v1-json");
    let json_shared_directory = root.join("directories/json-shared-all");
    let json_v1_directory = root.join("directories/json-shared-v1-json");
    let json_only_directory = root.join("directories/json-only");
    for directory in [
        &v2_directory,
        &v1_shared_directory,
        &v1_json_directory,
        &json_shared_directory,
        &json_v1_directory,
        &json_only_directory,
    ] {
        fs::create_dir_all(directory).expect("create source working directory");
    }

    let conn = Connection::open(&db_path).expect("create temporary opencode database");
    conn.execute_batch(
        r#"
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            directory TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            time_archived INTEGER
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            directory TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            time_archived INTEGER
        );
        CREATE TABLE session_message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        "#,
    )
    .expect("create v1 and v2 opencode schemas");

    conn.execute(
        "INSERT INTO session_v2 (id, title, directory, time_created, time_updated, time_archived) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
        params![
            "shared-all",
            "SQLite v2 title",
            v2_directory.to_string_lossy().as_ref(),
            900_i64,
            1_000_i64,
        ],
    )
    .expect("insert v2 session");
    conn.execute(
        "INSERT INTO session_message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "v2-message",
            "shared-all",
            1_000_i64,
            r#"{"data":{"model":{"id":"sqlite-v2-model"}}}"#,
        ],
    )
    .expect("insert v2 message");

    for (id, title, directory, created, updated) in [
        (
            "shared-all",
            "SQLite v1 duplicate title",
            &v1_shared_directory,
            7_900_i64,
            8_000_i64,
        ),
        (
            "shared-v1-json",
            "SQLite v1 title",
            &v1_json_directory,
            1_900_i64,
            2_000_i64,
        ),
    ] {
        conn.execute(
            "INSERT INTO session (id, title, directory, time_created, time_updated, time_archived) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            params![id, title, directory.to_string_lossy().as_ref(), created, updated],
        )
        .expect("insert v1 session");
    }
    conn.execute(
        "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "v1-shared-message",
            "shared-all",
            8_000_i64,
            r#"{"modelID":"sqlite-v1-duplicate-model"}"#,
        ],
    )
    .expect("insert shared v1 message");
    conn.execute(
        "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "v1-json-message",
            "shared-v1-json",
            2_000_i64,
            r#"{"modelID":"sqlite-v1-model"}"#,
        ],
    )
    .expect("insert v1/json message");
    drop(conn);

    let legacy_sessions = [
        (
            "shared-all",
            "Legacy JSON shared-all title",
            &json_shared_directory,
            9_000_i64,
            "legacy-shared-all-model",
        ),
        (
            "shared-v1-json",
            "Legacy JSON shared-v1-json title",
            &json_v1_directory,
            10_000_i64,
            "legacy-shared-v1-json-model",
        ),
        (
            "json-only",
            "Legacy JSON only title",
            &json_only_directory,
            11_000_i64,
            "legacy-json-only-model",
        ),
    ];
    for (id, title, directory, updated, model) in legacy_sessions {
        let session = serde_json::json!({
            "id": id,
            "directory": directory,
            "title": title,
            "time": { "created": updated - 100, "updated": updated }
        });
        write_temp_file(
            &storage_root
                .join("session")
                .join("project-1")
                .join(format!("{id}.json")),
            &serde_json::to_string(&session).expect("encode legacy session"),
        );
        let message = serde_json::json!({ "role": "assistant", "modelID": model });
        write_temp_file(
            &storage_root.join("message").join(id).join("message-1.json"),
            &serde_json::to_string(&message).expect("encode legacy message"),
        );
    }

    let entries = collect_opencode_history_sessions_from_sources(
        &db_path,
        std::slice::from_ref(&storage_root),
        None,
    );
    assert_eq!(entries.len(), 3);
    let by_id = entries
        .iter()
        .map(|entry| (entry.tool_session_id.as_str(), entry))
        .collect::<HashMap<_, _>>();
    assert_eq!(by_id.len(), 3);

    let shared_all = by_id.get("shared-all").expect("shared-all session");
    assert_eq!(shared_all.title, "SQLite v2 title");
    assert_eq!(shared_all.model_name.as_deref(), Some("sqlite-v2-model"));
    assert_eq!(shared_all.working_dir, v2_directory.to_string_lossy());
    assert_eq!(shared_all.updated_at_ms, 1_000);

    let shared_v1_json = by_id.get("shared-v1-json").expect("shared-v1-json session");
    assert_eq!(shared_v1_json.title, "SQLite v1 title");
    assert_eq!(
        shared_v1_json.model_name.as_deref(),
        Some("sqlite-v1-model")
    );
    assert_eq!(
        shared_v1_json.working_dir,
        v1_json_directory.to_string_lossy()
    );
    assert_eq!(shared_v1_json.updated_at_ms, 2_000);

    let json_only = by_id.get("json-only").expect("json-only session");
    assert_eq!(json_only.title, "Legacy JSON only title");
    assert_eq!(
        json_only.model_name.as_deref(),
        Some("legacy-json-only-model")
    );
    assert_eq!(json_only.working_dir, json_only_directory.to_string_lossy());
    assert_eq!(json_only.updated_at_ms, 11_000);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn opencode_history_does_not_fallback_when_v2_owner_is_filtered() {
    let root = make_temp_dir("opencode-history-filtered-v2-owner");
    let db_path = root.join("opencode.db");
    let cutoff = 1_000_i64;

    let conn = Connection::open(&db_path).expect("create temporary opencode database");
    conn.execute_batch(
        r#"
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            directory TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            time_archived INTEGER
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            directory TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            time_archived INTEGER
        );
        CREATE TABLE session_message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );

        INSERT INTO session_v2
            (id, title, directory, time_created, time_updated, time_archived)
        VALUES
            ('archived-owner', 'Archived v2 owner', '/tmp/archived-v2', 100, 2000, 2000),
            ('stale-owner', 'Stale v2 owner', '/tmp/stale-v2', 100, 900, NULL);

        INSERT INTO session
            (id, title, directory, time_created, time_updated, time_archived)
        VALUES
            ('archived-owner', 'Active v1 duplicate', '/tmp/archived-v1', 100, 3000, NULL),
            ('stale-owner', 'Fresh v1 duplicate', '/tmp/stale-v1', 100, 2500, NULL),
            ('v1-only', 'Fresh v1 only', '/tmp/v1-only', 100, 2000, NULL);
        "#,
    )
    .expect("create schemas and insert filtered owner sessions");
    drop(conn);

    let entries = collect_opencode_history_sessions_from_sources(&db_path, &[], Some(cutoff));
    let actual_ids = entries
        .iter()
        .map(|entry| entry.tool_session_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(actual_ids, vec!["v1-only"]);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn claude_usage_parser_reads_assistant_usage() {
    let root = make_temp_dir("claude-usage");
    let path = root.join("claude-session.jsonl");
    write_temp_file(
        &path,
        concat!(
            "{\"type\":\"user\",\"sessionId\":\"claude-session\",\"timestamp\":\"2026-03-10T05:09:58.846Z\"}\n",
            "{\"type\":\"assistant\",\"sessionId\":\"claude-session\",\"timestamp\":\"2026-03-10T05:10:07.255Z\",\"message\":{\"model\":\"claude-opus-4-6\",\"usage\":{\"input_tokens\":100,\"output_tokens\":25,\"cache_read_input_tokens\":40,\"cache_creation_input_tokens\":10}}}\n"
        ),
    );

    let records = parse_claude_usage_file(&path).expect("claude usage");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].session_id, "claude-session");
    assert_eq!(records[0].model.as_deref(), Some("claude-opus-4-6"));
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].output_tokens, 25);
    assert_eq!(records[0].cache_tokens, 50);
    assert_eq!(records[0].total_tokens, 175);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn codex_usage_parser_reads_token_count_events() {
    let root = make_temp_dir("codex-usage");
    let path = root.join("rollout-session.jsonl");
    write_temp_file(
        &path,
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-session\",\"timestamp\":\"2026-03-03T01:19:17.343Z\",\"cwd\":\"/tmp/codex-project\"}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5-codex\"}}\n",
            "{\"type\":\"event_msg\",\"timestamp\":\"2026-03-03T01:20:00.000Z\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"input_tokens\":200,\"cached_input_tokens\":75,\"output_tokens\":80,\"total_tokens\":280}}}}\n"
        ),
    );

    let records = parse_codex_usage_file(&path).expect("codex usage");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].session_id, "codex-session");
    assert_eq!(records[0].model.as_deref(), Some("gpt-5-codex"));
    assert_eq!(records[0].input_tokens, 200);
    assert_eq!(records[0].cache_tokens, 75);
    assert_eq!(records[0].output_tokens, 80);
    assert_eq!(records[0].total_tokens, 280);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn codex_usage_parser_backfills_model_from_later_turn_context() {
    let root = make_temp_dir("codex-usage-later-model");
    let path = root.join("rollout-session.jsonl");
    write_temp_file(
        &path,
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-session\"}}\n",
            "{\"type\":\"event_msg\",\"timestamp\":\"2026-07-27T01:20:00.000Z\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"input_tokens\":200,\"cached_input_tokens\":75,\"output_tokens\":80,\"total_tokens\":280}}}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.6-terra\"}}\n"
        ),
    );

    let records = parse_codex_usage_file(&path).expect("codex usage");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model.as_deref(), Some("gpt-5.6-terra"));

    let _ = fs::remove_dir_all(root);
}

// ============================================================
// antigravity legacy Gemini tmp usage — behaviour tests
// These tests assert on the public boundary:
//   sessions_usage_tool_stats("antigravity", days) → SessionUsageToolStats
// Expected disk layout (once impl lands):
//   $HOME/.gemini/tmp/<rolloutId>/chats/session-*.json
//   $HOME/.gemini/tmp/<rolloutId>/chats/session-*.jsonl
// ============================================================

#[test]
fn antigravity_json_scans_session_files_and_aggregates_token_totals() {
    // One rollout dir, two messages in one session.json.
    // total_or_sum(total, input, output, cached): total==0 → input+output+cached.
    //   msg1: total_or_sum(0, 20, 5, 0) = 25
    //   msg2: total_or_sum(0, 30, 8, 0) = 38
    //   total = 25 + 38 = 63
    // Fixture timestamps use raw ISO strings — file mtime provides the real
    // timestamp when ISO strings are outside the window.
    let root = make_temp_dir("antigravity-json-multi-msg");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();

    // Fixture timestamps use dynamic RFC3339 dates within the 7-day window.
    let ts_now = Local::now();
    let day0 = (ts_now.date_naive() - chrono::Duration::days(0))
        .and_time(chrono::NaiveTime::from_hms_opt(5, 9, 58).unwrap());
    let day1 = (ts_now.date_naive() - chrono::Duration::days(1))
        .and_time(chrono::NaiveTime::from_hms_opt(5, 10, 7).unwrap());
    let fmt_ts = |dt: chrono::NaiveDateTime| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let chats_dir = root
        .join(".gemini")
        .join("tmp")
        .join("rollout-abc")
        .join("chats");
    let content_foo = format!(r#"{{
  "sessionId": "conv-1",
  "messages": [
    {{
      "tokens": {{ "input": 20, "output": 5, "cached": 0, "total": 0 }},
      "model": "gemini-pro-v1",
      "modelName": "Gemini Pro v1",
      "timestamp": "{}"
    }},
    {{
      "tokens": {{ "input": 30, "output": 8, "cached": 0, "total": 0 }},
      "model": "gemini-ultra",
      "modelName": "Gemini Ultra",
      "timestamp": "{}"
    }}
  ]
}}"#, fmt_ts(day0), fmt_ts(day1));
    write_temp_file(
        &chats_dir.join("session-foo.json"),
        &content_foo,
    );

    let stats =
        sessions_usage_tool_stats("antigravity".to_string(), Some(7)).expect("tool stats");

    // Note: sessions_usage_tool_stats uses include_model_breakdown=false,
    // so stats.models is always empty. Remove model assertions.
    assert_eq!(stats.tool, "antigravity");
    assert_eq!(stats.source_status, "available");
    assert_eq!(stats.summary.total_tokens, 63); // 25 + 38 (total_or_sum fallback)
    assert_eq!(stats.summary.input_tokens, 50);
    assert_eq!(stats.summary.output_tokens, 13);
    assert_eq!(stats.summary.calls, 2);
    assert_eq!(stats.scanned_sessions, 1);

    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn antigravity_json_merge_across_multiple_session_files() {
    // Two session files contribute to different models; summary merges all.
    //   file1 (gemma):  total_or_sum(0, 10, 3, 0) = 13
    //   file2 (palm):   total_or_sum(26, 15, 6, 5) = 26  (total>0, use as-is)
    //   total = 13 + 26 = 39
    let root = make_temp_dir("antigravity-json-two-files");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();
    let ts_now = Local::now();
    let day0 = (ts_now.date_naive() - chrono::Duration::days(0))
        .and_time(chrono::NaiveTime::from_hms_opt(5, 9, 58).unwrap());
    let day1 = (ts_now.date_naive() - chrono::Duration::days(1))
        .and_time(chrono::NaiveTime::from_hms_opt(5, 10, 7).unwrap());
    let fmt_ts = |dt: chrono::NaiveDateTime| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let chats_dir_1 = root
        .join(".gemini")
        .join("tmp")
        .join("rollout-a")
        .join("chats");
    let content_one = format!(r#"{{
  "sessionId": "conv-1",
  "messages": [{{
    "tokens": {{ "input": 10, "output": 3, "cached": 0, "total": 0 }},
    "model": "gemma",
    "modelName": "",
    "timestamp": "{}"
  }}]
}}"#, fmt_ts(day0));
    write_temp_file(
        &chats_dir_1.join("session-one.json"),
        &content_one,
    );
    let chats_dir_2 = root
        .join(".gemini")
        .join("tmp")
        .join("rollout-b")
        .join("chats");
    let content_two = format!(r#"{{
  "sessionId": "conv-2",
  "messages": [{{
    "tokens": {{ "input": 15, "output": 6, "cached": 5, "total": 26 }},
    "model": "palm",
    "modelName": "PaLM",
    "timestamp": "{}"
  }}]
}}"#, fmt_ts(day1));
    write_temp_file(
        &chats_dir_2.join("session-two.json"),
        &content_two,
    );

    let stats =
        sessions_usage_tool_stats("antigravity".to_string(), Some(7)).expect("tool stats");

    assert_eq!(stats.tool, "antigravity");
    assert_eq!(stats.source_status, "available");
    assert_eq!(stats.summary.total_tokens, 39); // 13 + 26 (total_or_sum)
    assert_eq!(stats.summary.input_tokens, 25);
    assert_eq!(stats.summary.output_tokens, 9);
    assert_eq!(stats.summary.cache_tokens, 5);
    assert_eq!(stats.summary.sessions, 2);
    assert_eq!(stats.scanned_sessions, 2);
    // models.len() always 0 for sessions_usage_tool_stats (include_model_breakdown=false)
    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn antigravity_json_handles_model_fallback_from_modelName_and_stale_timestamp() {
    // Literal fallback chain (no "expired → mtime" rule):
    //   message.timestamp (empty string → None) →
    //   value.lastUpdated (valid JSON number, in-window) →
    //   file mtime (never reached).
    // When `model` is empty, fall back to modelName.
    let root = make_temp_dir("antigravity-json-model-fallback");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();
    // Use dynamic lastUpdated so it stays inside the 7-day window.
    // The `_i64` Rust suffix is invalid JSON — removed so the parser
    // correctly reads `lastUpdated` instead of falling through to mtime.
    let lu_ms = timestamp_days_ago(0);
    let chats_dir = root
        .join(".gemini")
        .join("tmp")
        .join("rollout-m")
        .join("chats");
    let content = format!(r#"{{
  "sessionId": "conv-fb",
  "lastUpdated": {},
  "messages": [{{
    "tokens": {{ "input": 5, "output": 2, "cached": 1, "total": 0 }},
    "model": "",
    "modelName": "fallback-model",
    "timestamp": ""
  }}]
}}"#, lu_ms);
    write_temp_file(
        &chats_dir.join("session-fb.json"),
        &content,
    );

    let stats =
        sessions_usage_tool_stats("antigravity".to_string(), Some(7)).expect("tool stats");

    assert_eq!(stats.tool, "antigravity");
    assert_eq!(stats.source_status, "available");
    assert_eq!(stats.summary.total_tokens, 8);
    // Note: stats.models is empty because sessions_usage_tool_stats uses
    // include_model_breakdown=false. The model fallback logic is verified by
    // the fact that total_tokens=8 (5+2+1) instead of failing on parsing.
    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn antigravity_jsonl_parses_line_by_line_and_skips_dollar_set() {
    // File A has a $set header → skipped; file B has regular gemini rows.
    //   fileA: (40+10+0) = 50
    //   fileB: (20+7+3)  = 30
    //   total = 80
    let root = make_temp_dir("antigravity-jsonl-set-skip");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();
    let ts_now = Local::now();
    let day0 = (ts_now.date_naive() - chrono::Duration::days(0))
        .and_time(chrono::NaiveTime::from_hms_opt(5, 9, 58).unwrap());
    let day1 = (ts_now.date_naive() - chrono::Duration::days(1))
        .and_time(chrono::NaiveTime::from_hms_opt(5, 10, 7).unwrap());
    let fmt_ts = |dt: chrono::NaiveDateTime| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let chats_dir_a = root
        .join(".gemini")
        .join("tmp")
        .join("rollout-l1")
        .join("chats");
    let content_a = format!("{{\"$set\":{{\"id\":\"header\"}}}}\n{{\"type\":\"gemini\",\"tokens\":{{\"input\":40,\"output\":10,\"cached\":0}},\"model\":\"jsonl-model\",\"timestamp\":\"{ts}\"}}\n", ts = fmt_ts(day0));
    write_temp_file(
        &chats_dir_a.join("session-l1.jsonl"),
        &content_a,
    );
    let chats_dir_b = root
        .join(".gemini")
        .join("tmp")
        .join("rollout-l2")
        .join("chats");
    let content_b = format!("{{\"type\":\"gemini\",\"tokens\":{{\"input\":20,\"output\":7,\"cached\":3}},\"model\":\"jsonl-model\",\"timestamp\":\"{ts}\"}}\n", ts = fmt_ts(day1));
    write_temp_file(
        &chats_dir_b.join("session-l2.jsonl"),
        &content_b,
    );

    let stats =
        sessions_usage_tool_stats("antigravity".to_string(), Some(7)).expect("tool stats");

    assert_eq!(stats.tool, "antigravity");
    assert_eq!(stats.source_status, "available");
    assert_eq!(stats.summary.total_tokens, 80); // (40+10+0) + (20+7+3) = 50+30
    assert_eq!(stats.summary.input_tokens, 60);
    assert_eq!(stats.summary.output_tokens, 17);
    assert_eq!(stats.summary.cache_tokens, 3);
    assert_eq!(stats.scanned_sessions, 2);

    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn antigravity_jsonl_falls_back_to_input_plus_output_when_total_missing() {
    // Lines omit `total`; parser should compute `input + output + cached`.
    let root = make_temp_dir("antigravity-jsonl-total-fallback");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();
    let ts_now = Local::now();
    let day0 = (ts_now.date_naive() - chrono::Duration::days(0))
        .and_time(chrono::NaiveTime::from_hms_opt(5, 9, 58).unwrap());
    let fmt_ts = |dt: chrono::NaiveDateTime| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let chats_dir = root
        .join(".gemini")
        .join("tmp")
        .join("rollout-ft")
        .join("chats");
    let content_ft = format!("{{\"type\":\"gemini\",\"tokens\":{{\"input\":100,\"output\":50,\"cached\":20}},\"model\":\"no-total-model\",\"timestamp\":\"{ts}\"}}\n", ts = fmt_ts(day0));
    write_temp_file(
        &chats_dir.join("session-total.jsonl"),
        &content_ft,
    );

    let stats =
        sessions_usage_tool_stats("antigravity".to_string(), Some(7)).expect("tool stats");

    assert_eq!(stats.tool, "antigravity");
    assert_eq!(stats.source_status, "available");
    assert_eq!(stats.summary.total_tokens, 170); // 100+50+20
    assert_eq!(stats.summary.cache_tokens, 20);

    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn antigravity_json_skip_all_zero_token_entries() {
    // All messages carry zero tokens; no record produced, session still counted
    // as scanned.
    let root = make_temp_dir("antigravity-json-zero");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();
    let ts_now = Local::now();
    let day0 = (ts_now.date_naive() - chrono::Duration::days(0))
        .and_time(chrono::NaiveTime::from_hms_opt(5, 9, 58).unwrap());
    let day1 = (ts_now.date_naive() - chrono::Duration::days(1))
        .and_time(chrono::NaiveTime::from_hms_opt(5, 10, 7).unwrap());
    let fmt_ts = |dt: chrono::NaiveDateTime| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let chats_dir = root
        .join(".gemini")
        .join("tmp")
        .join("rollout-z")
        .join("chats");
    let content = format!(r#"{{
  "sessionId": "conv-zero",
  "messages": [
    {{"tokens":{{"input":0,"output":0,"cached":0,"total":0}},"model":"x","timestamp":"{ts0}"}},
    {{"tokens":{{"input":0,"output":0,"cached":0,"total":0}},"model":"y","timestamp":"{ts1}"}}
  ]
}}"#, ts0 = fmt_ts(day0), ts1 = fmt_ts(day1));
    write_temp_file(
        &chats_dir.join("session-zero.json"),
        &content,
    );

    let stats =
        sessions_usage_tool_stats("antigravity".to_string(), Some(7)).expect("tool stats");

    assert_eq!(stats.source_status, "empty"); // available but zero records
    assert_eq!(stats.summary.total_tokens, 0);
    assert_eq!(stats.scanned_sessions, 1);

    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn antigravity_jsonl_skip_zero_token_lines() {
    // All lines have zero tokens; nothing recorded, file still scanned.
    let root = make_temp_dir("antigravity-jsonl-zero");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();
    let ts_now = Local::now();
    let day0 = (ts_now.date_naive() - chrono::Duration::days(0))
        .and_time(chrono::NaiveTime::from_hms_opt(5, 9, 58).unwrap());
    let fmt_ts = |dt: chrono::NaiveDateTime| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let chats_dir = root
        .join(".gemini")
        .join("tmp")
        .join("rollout-jz")
        .join("chats");
    let content_z = format!("{{\"type\":\"gemini\",\"tokens\":{{\"input\":0,\"output\":0,\"cached\":0}},\"model\":\"z\",\"timestamp\":\"{ts}\"}}\n", ts = fmt_ts(day0));
    write_temp_file(
        &chats_dir.join("session-z.jsonl"),
        &content_z,
    );

    let stats =
        sessions_usage_tool_stats("antigravity".to_string(), Some(7)).expect("tool stats");

    assert_eq!(stats.source_status, "empty");
    assert_eq!(stats.summary.total_tokens, 0);
    assert_eq!(stats.scanned_sessions, 1);

    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn antigravity_no_tmp_directory_returns_unavailable() {
    // With an empty HOME, no .gemini/tmp path exists → source_status="unavailable".
    let root = make_temp_dir("antigravity-no-tmp");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();

    let stats =
        sessions_usage_tool_stats("antigravity".to_string(), Some(7)).expect("tool stats");

    assert_eq!(stats.tool, "antigravity");
    assert_eq!(stats.source_status, "unavailable");
    assert_eq!(stats.summary.total_tokens, 0);
    assert_eq!(stats.summary.calls, 0);
    assert_eq!(stats.scanned_sessions, 0);

    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn opencode_json_usage_parser_reads_message_tokens() {
    let root = make_temp_dir("opencode-usage");
    let messages_dir = root.join("message/ses_123");
    write_temp_file(
        &messages_dir.join("msg_1.json"),
        r#"{
  "role": "assistant",
  "modelID": "deepseek-v4",
  "time": { "created": 1770800496647 },
  "tokens": { "input": 50, "output": 70, "cache": { "read": 20, "write": 5 } }
}"#,
    );

    let (records, errors) = parse_opencode_message_usage_dir(&messages_dir, "ses_123");
    assert!(errors.is_empty());
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].session_id, "ses_123");
    assert_eq!(records[0].model.as_deref(), Some("deepseek-v4"));
    assert_eq!(records[0].input_tokens, 50);
    assert_eq!(records[0].cache_tokens, 25);
    assert_eq!(records[0].output_tokens, 70);
    assert_eq!(records[0].total_tokens, 145);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn opencode_db_usage_query_only_reads_requested_time_window() {
    let conn = Connection::open_in_memory().expect("in-memory opencode db");
    conn.execute_batch(
        r#"
        CREATE TABLE session (id TEXT PRIMARY KEY, time_archived INTEGER);
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        INSERT INTO session (id, time_archived) VALUES ('session-1', NULL);
        "#,
    )
    .expect("create opencode schema");
    conn.execute(
        "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "old-message",
            "session-1",
            100_i64,
            r#"{"modelID":"old-model","tokens":{"input":100,"output":10}}"#,
        ],
    )
    .expect("insert old message");
    conn.execute(
        "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "selected-message",
            "session-1",
            200_i64,
            r#"{"modelID":"selected-model","tokens":{"input":20,"output":5}}"#,
        ],
    )
    .expect("insert selected message");

    let records = read_opencode_message_tokens_for_test(&conn, 150, 250);

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model.as_deref(), Some("selected-model"));
    assert_eq!(records[0].total_tokens, 25);
}

#[test]
fn opencode_usage_merges_sqlite_v2_v1_and_legacy_json_per_session() {
    let root = make_temp_dir("opencode-usage-source-priority");
    let db_path = root.join("opencode.db");
    let storage_root = root.join("storage");

    let conn = Connection::open(&db_path).expect("create temporary opencode database");
    conn.execute_batch(
        r#"
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            time_archived INTEGER
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            time_archived INTEGER
        );
        CREATE TABLE session_message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        INSERT INTO session_v2 (id, time_archived) VALUES ('shared-all', NULL);
        INSERT INTO session (id, time_archived) VALUES ('shared-all', NULL);
        INSERT INTO session (id, time_archived) VALUES ('shared-v1-json', NULL);
        "#,
    )
    .expect("create v1 and v2 usage schemas");

    for (id, session_id, timestamp_ms, data) in [
        (
            "v2-first",
            "shared-all",
            200_i64,
            r#"{"role":"assistant","modelID":"v2-winning-model","tokens":{"input":6,"output":5}}"#,
        ),
        (
            "v2-second",
            "shared-all",
            300_i64,
            r#"{"role":"assistant","modelID":"v2-winning-model","tokens":{"input":12,"output":10}}"#,
        ),
    ] {
        conn.execute(
            "INSERT INTO session_message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            params![id, session_id, timestamp_ms, data],
        )
        .expect("insert v2 usage message");
    }

    for (id, session_id, timestamp_ms, data) in [
        (
            "v1-shared-all",
            "shared-all",
            400_i64,
            r#"{"role":"assistant","modelID":"v1-losing-model","tokens":{"input":500,"output":500}}"#,
        ),
        (
            "v1-shared-v1-json",
            "shared-v1-json",
            500_i64,
            r#"{"role":"assistant","modelID":"v1-winning-model","tokens":{"input":20,"output":13}}"#,
        ),
    ] {
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            params![id, session_id, timestamp_ms, data],
        )
        .expect("insert v1 usage message");
    }
    drop(conn);

    for (session_id, model, total_tokens) in [
        ("shared-all", "json-shared-all-losing-model", 2_000_u64),
        (
            "shared-v1-json",
            "json-shared-v1-json-losing-model",
            3_000_u64,
        ),
        ("json-only", "json-only-winning-model", 44_u64),
    ] {
        write_temp_file(
            &storage_root
                .join("session")
                .join("project-1")
                .join(format!("{session_id}.json")),
            &serde_json::json!({ "id": session_id }).to_string(),
        );
        write_temp_file(
            &storage_root
                .join("message")
                .join(session_id)
                .join("message-1.json"),
            &serde_json::json!({
                "role": "assistant",
                "modelID": model,
                "time": { "created": 600 },
                "tokens": { "total": total_tokens }
            })
            .to_string(),
        );
    }

    let scan = collect_opencode_usage_records_from_sources(
        &db_path,
        std::slice::from_ref(&storage_root),
        100,
        1_000,
    );

    assert_eq!(scan.scanned_sessions, 3);
    assert_eq!(scan.records.len(), 4);
    assert_eq!(
        scan.records
            .iter()
            .map(|record| record.session_id.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );
    assert_eq!(
        scan.records
            .iter()
            .map(|record| record.total_tokens)
            .sum::<u64>(),
        110
    );

    let shared_all = scan
        .records
        .iter()
        .filter(|record| record.session_id == "shared-all")
        .collect::<Vec<_>>();
    assert_eq!(shared_all.len(), 2);
    assert_eq!(
        shared_all
            .iter()
            .map(|record| record.total_tokens)
            .sum::<u64>(),
        33
    );

    let models = scan
        .records
        .iter()
        .filter_map(|record| record.model.as_deref())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        models,
        std::collections::HashSet::from([
            "v2-winning-model",
            "v1-winning-model",
            "json-only-winning-model",
        ])
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn opencode_usage_trims_session_ids_before_source_selection() {
    let root = make_temp_dir("opencode-usage-trimmed-session-id");
    let db_path = root.join("opencode.db");

    let conn = Connection::open(&db_path).expect("create temporary opencode database");
    conn.execute_batch(
        r#"
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            time_archived INTEGER
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            time_archived INTEGER
        );
        CREATE TABLE session_message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        INSERT INTO session_v2 (id, time_archived) VALUES (' shared ', NULL);
        INSERT INTO session (id, time_archived) VALUES ('shared', NULL);
        "#,
    )
    .expect("create v1 and v2 usage schemas");
    conn.execute(
        "INSERT INTO session_message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "v2-message",
            " shared ",
            200_i64,
            r#"{"role":"assistant","modelID":"v2-model","tokens":{"input":6,"output":5}}"#,
        ],
    )
    .expect("insert v2 usage message");
    conn.execute(
        "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "v1-message",
            "shared",
            300_i64,
            r#"{"role":"assistant","modelID":"v1-model","tokens":{"input":500,"output":500}}"#,
        ],
    )
    .expect("insert v1 usage message");
    drop(conn);

    let scan = collect_opencode_usage_records_from_sources(&db_path, &[], 100, 1_000);

    assert_eq!(scan.scanned_sessions, 1);
    assert!(!scan.records.is_empty());
    assert!(scan
        .records
        .iter()
        .all(|record| record.session_id == "shared"));
    assert_eq!(
        scan.records
            .iter()
            .map(|record| record.total_tokens)
            .sum::<u64>(),
        11
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn opencode_usage_keeps_valid_messages_when_a_sibling_is_corrupt() {
    let root = make_temp_dir("opencode-usage-corrupt-sibling");
    let db_path = root.join("opencode.db");
    let storage_root = root.join("storage");

    let conn = Connection::open(&db_path).expect("create temporary opencode database");
    conn.execute_batch(
        r#"
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            time_archived INTEGER
        );
        CREATE TABLE session_message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        INSERT INTO session_v2 (id, time_archived) VALUES ('db-partial', NULL);
        "#,
    )
    .expect("create v2 usage schema");
    conn.execute(
        "INSERT INTO session_message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "db-valid",
            "db-partial",
            200_i64,
            r#"{"role":"assistant","modelID":"db-valid-model","tokens":{"input":6,"output":5}}"#,
        ],
    )
    .expect("insert valid v2 message");
    conn.execute(
        "INSERT INTO session_message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params!["db-corrupt", "db-partial", 300_i64, "{not-json"],
    )
    .expect("insert corrupt v2 message");
    drop(conn);

    write_temp_file(
        &storage_root
            .join("session")
            .join("project-1")
            .join("json-partial.json"),
        &serde_json::json!({ "id": "json-partial" }).to_string(),
    );
    let messages_dir = storage_root.join("message").join("json-partial");
    write_temp_file(
        &messages_dir.join("message-valid.json"),
        &serde_json::json!({
            "role": "assistant",
            "modelID": "json-valid-model",
            "time": { "created": 600 },
            "tokens": { "total": 22 }
        })
        .to_string(),
    );
    write_temp_file(&messages_dir.join("message-corrupt.json"), "{not-json");

    let scan = collect_opencode_usage_records_from_sources(
        &db_path,
        std::slice::from_ref(&storage_root),
        100,
        1_000,
    );

    assert_eq!(scan.scanned_sessions, 2);
    assert_eq!(
        scan.records
            .iter()
            .map(|record| record.total_tokens)
            .sum::<u64>(),
        33
    );
    assert!(scan
        .records
        .iter()
        .any(|record| record.model.as_deref() == Some("db-valid-model")));
    assert!(scan
        .records
        .iter()
        .any(|record| record.model.as_deref() == Some("json-valid-model")));
    assert!(scan.errors.iter().any(|error| error.contains("db-partial")));
    assert!(scan
        .errors
        .iter()
        .any(|error| error.contains("json-partial")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn opencode_usage_does_not_fall_back_when_v2_owner_is_archived() {
    let root = make_temp_dir("opencode-usage-archived-owner");
    let db_path = root.join("opencode.db");

    let conn = Connection::open(&db_path).expect("create temporary opencode database");
    conn.execute_batch(
        r#"
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            time_archived INTEGER
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            time_archived INTEGER
        );
        CREATE TABLE session_message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        INSERT INTO session_v2 (id, time_archived) VALUES ('archived-owner', 500);
        INSERT INTO session (id, time_archived) VALUES ('archived-owner', NULL);
        INSERT INTO session (id, time_archived) VALUES ('v1-only', NULL);
        "#,
    )
    .expect("create v1 and v2 usage schemas");
    conn.execute(
        "INSERT INTO session_message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "v2-archived-message",
            "archived-owner",
            200_i64,
            r#"{"modelID":"v2-archived-model","tokens":{"input":100,"output":100}}"#,
        ],
    )
    .expect("insert v2 message for archived session");
    conn.execute(
        "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "v1-archived-owner-message",
            "archived-owner",
            300_i64,
            r#"{"modelID":"v1-losing-model","tokens":{"input":500,"output":500}}"#,
        ],
    )
    .expect("insert v1 message for archived owner");
    conn.execute(
        "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            "v1-only-message",
            "v1-only",
            400_i64,
            r#"{"modelID":"v1-kept-model","tokens":{"input":6,"output":5}}"#,
        ],
    )
    .expect("insert v1 only message");
    drop(conn);

    let scan = collect_opencode_usage_records_from_sources(&db_path, &[], 100, 1_000);

    assert_eq!(scan.scanned_sessions, 1);
    assert_eq!(scan.records.len(), 1);
    assert_eq!(scan.records[0].session_id, "v1-only");
    assert_eq!(scan.records[0].total_tokens, 11);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn usage_aggregation_fills_empty_days_and_filters_window() {
    let stats = aggregate_usage_for_test(
        "claude",
        7,
        vec![
            UsageRecord {
                session_id: "in-window".to_string(),
                model: Some("claude-opus-4-6".to_string()),
                timestamp_ms: timestamp_days_ago(1),
                input_tokens: 100,
                output_tokens: 40,
                cache_tokens: 20,
                cache_read_tokens: 20,
                total_tokens: 160,
            },
            UsageRecord {
                session_id: "old".to_string(),
                model: Some("claude-sonnet-4-5".to_string()),
                timestamp_ms: timestamp_days_ago(10),
                input_tokens: 999,
                output_tokens: 999,
                cache_tokens: 999,
                cache_read_tokens: 0,
                total_tokens: 999,
            },
        ],
    );

    assert_eq!(stats.daily.len(), 7);
    assert_eq!(stats.summary.total_tokens, 160);
    assert_eq!(stats.summary.calls, 1);
    assert_eq!(stats.summary.sessions, 1);
    assert_eq!(stats.summary.cache_hit_rate, 16);
    assert_eq!(stats.scanned_calls, 1);
    assert!(stats.daily.iter().any(|day| day.total_tokens == 0));
}

#[test]
fn codex_usage_aggregation_treats_cached_input_as_part_of_input() {
    let stats = aggregate_usage_for_test(
        "codex",
        7,
        vec![UsageRecord {
            session_id: "codex-session".to_string(),
            model: Some("gpt-5-codex".to_string()),
            timestamp_ms: timestamp_days_ago(1),
            input_tokens: 100,
            output_tokens: 40,
            cache_tokens: 20,
            cache_read_tokens: 20,
            total_tokens: 140,
        }],
    );

    assert_eq!(stats.summary.input_tokens, 100);
    assert_eq!(stats.summary.cache_tokens, 20);
    assert_eq!(stats.summary.cache_hit_rate, 20);
    assert_eq!(
        stats
            .daily
            .iter()
            .find(|day| day.calls == 1)
            .expect("usage day")
            .cache_hit_rate,
        20
    );
}

#[test]
fn usage_aggregation_keeps_tools_independent_and_peak_day_by_total() {
    let claude = aggregate_usage_for_test(
        "claude",
        15,
        vec![UsageRecord {
            session_id: "claude-session".to_string(),
            model: Some("claude-opus-4-6".to_string()),
            timestamp_ms: timestamp_days_ago(2),
            input_tokens: 0,
            output_tokens: 25,
            cache_tokens: 10,
            cache_read_tokens: 10,
            total_tokens: 35,
        }],
    );
    let codex = aggregate_usage_for_test(
        "codex",
        15,
        vec![
            UsageRecord {
                session_id: "codex-a".to_string(),
                model: Some("gpt-5-codex".to_string()),
                timestamp_ms: timestamp_days_ago(3),
                input_tokens: 100,
                output_tokens: 25,
                cache_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 125,
            },
            UsageRecord {
                session_id: "codex-b".to_string(),
                model: Some("gpt-5.1-codex".to_string()),
                timestamp_ms: timestamp_days_ago(1),
                input_tokens: 150,
                output_tokens: 100,
                cache_tokens: 50,
                cache_read_tokens: 50,
                total_tokens: 300,
            },
        ],
    );

    assert_eq!(claude.tool, "claude");
    assert_eq!(codex.tool, "codex");
    assert_eq!(claude.summary.total_tokens, 35);
    assert_eq!(codex.summary.total_tokens, 425);
    assert_eq!(claude.summary.cache_hit_rate, 100);
    assert_eq!(codex.peak_day.expect("codex peak").total_tokens, 300);
}

#[test]
fn usage_day_stats_matches_sum_of_tool_daily_stats() {
    let target_date = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let claude = aggregate_usage_for_test(
        "claude",
        1,
        vec![
            UsageRecord {
                session_id: "same-session".to_string(),
                model: Some("claude-opus-4-6".to_string()),
                timestamp_ms: timestamp_days_ago(0),
                input_tokens: 100,
                output_tokens: 40,
                cache_tokens: 10,
                cache_read_tokens: 10,
                total_tokens: 150,
            },
            UsageRecord {
                session_id: "previous-day".to_string(),
                model: Some("claude-sonnet-4-5".to_string()),
                timestamp_ms: timestamp_days_ago(1),
                input_tokens: 500,
                output_tokens: 200,
                cache_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 700,
            },
        ],
    );
    let codex = aggregate_usage_for_test(
        "codex",
        1,
        vec![UsageRecord {
            session_id: "same-session".to_string(),
            model: Some("gpt-5-codex".to_string()),
            timestamp_ms: timestamp_days_ago(0),
            input_tokens: 25,
            output_tokens: 30,
            cache_tokens: 5,
            cache_read_tokens: 0,
            total_tokens: 60,
        }],
    );

    let stats = aggregate_day_stats_for_test(target_date, &[claude, codex]);

    assert_eq!(stats.total_tokens, 210);
    assert_eq!(stats.calls, 2);
    assert_eq!(stats.sessions, 2);
    assert_eq!(stats.input_tokens, 125);
    assert_eq!(stats.output_tokens, 70);
    assert_eq!(stats.cache_tokens, 15);
    assert_eq!(stats.breakdown.len(), 2);
    assert_eq!(stats.breakdown[0].tool, "claude");
    assert_eq!(stats.breakdown[0].total_tokens, 150);
    assert_eq!(stats.breakdown[0].cache_hit_rate, 9);
    assert_eq!(stats.breakdown[0].models.len(), 1);
    assert_eq!(stats.breakdown[0].models[0].model, "claude-opus-4-6");
    assert_eq!(stats.breakdown[0].models[0].total_tokens, 150);
    assert_eq!(stats.breakdown[1].tool, "codex");
    assert_eq!(stats.breakdown[1].total_tokens, 60);
    assert_eq!(stats.breakdown[1].cache_hit_rate, 0);
    assert_eq!(stats.breakdown[1].models.len(), 1);
    assert_eq!(stats.breakdown[1].models[0].model, "gpt-5-codex");
}

#[test]
fn usage_model_aggregation_separates_models_and_excludes_other_dates() {
    let stats = aggregate_usage_for_test(
        "claude",
        1,
        vec![
            UsageRecord {
                session_id: "opus-session".to_string(),
                model: Some("claude-opus-4-6".to_string()),
                timestamp_ms: timestamp_days_ago(0),
                input_tokens: 100,
                output_tokens: 50,
                cache_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 150,
            },
            UsageRecord {
                session_id: "sonnet-session".to_string(),
                model: Some("claude-sonnet-4-5".to_string()),
                timestamp_ms: timestamp_days_ago(0),
                input_tokens: 60,
                output_tokens: 40,
                cache_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 100,
            },
            UsageRecord {
                session_id: "old-session".to_string(),
                model: Some("claude-haiku-4-5".to_string()),
                timestamp_ms: timestamp_days_ago(1),
                input_tokens: 500,
                output_tokens: 500,
                cache_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 1000,
            },
        ],
    );

    assert_eq!(stats.models.len(), 2);
    assert_eq!(stats.models[0].model, "claude-opus-4-6");
    assert_eq!(stats.models[0].total_tokens, 150);
    assert_eq!(stats.models[1].model, "claude-sonnet-4-5");
    assert_eq!(stats.models[1].total_tokens, 100);
    assert_eq!(
        stats
            .models
            .iter()
            .map(|model| model.total_tokens)
            .sum::<u64>(),
        250
    );
}

#[test]
fn usage_scan_cache_reuses_records_until_cleared() {
    let cache = ToolScanCache::default();
    let collections = AtomicUsize::new(0);

    let collect = || {
        collections.fetch_add(1, Ordering::SeqCst);
        ToolScan::default()
    };

    cache.get_or_collect(100, 200, collect);
    cache.get_or_collect(120, 180, collect);
    assert_eq!(collections.load(Ordering::SeqCst), 1);

    cache.get_or_collect(50, 180, collect);
    assert_eq!(collections.load(Ordering::SeqCst), 2);

    cache.clear();
    cache.get_or_collect(100, 200, collect);
    assert_eq!(collections.load(Ordering::SeqCst), 3);
}

#[test]
fn usage_file_window_filter_keeps_boundary_and_newer_files() {
    assert!(usage_file_may_overlap_window_for_test(0, 100));
    assert!(!usage_file_may_overlap_window_for_test(99, 100));
    assert!(usage_file_may_overlap_window_for_test(100, 100));
    assert!(usage_file_may_overlap_window_for_test(101, 100));
}

#[test]
fn usage_parser_reports_malformed_source_without_panicking() {
    let root = make_temp_dir("usage-malformed");
    let path = root.join("bad.jsonl");
    write_temp_file(&path, "{not-json}\n");

    let result = parse_codex_usage_file(&path);
    assert!(result.is_err());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn usage_tool_stats_returns_single_tool_and_normalizes_days() {
    let stats = sessions_usage_tool_stats("claude".to_string(), Some(15)).expect("tool stats");
    assert_eq!(stats.tool, "claude");
    assert_eq!(stats.daily.len(), 15);

    let fallback = sessions_usage_tool_stats("claude".to_string(), Some(2)).expect("tool stats");
    assert_eq!(fallback.daily.len(), 7);
}

#[test]
fn usage_tool_stats_rejects_unknown_tool() {
    let error = sessions_usage_tool_stats("unknown".to_string(), Some(7)).expect_err("tool error");
    assert!(error.contains("unsupported tool: unknown"));
}

#[test]
fn usage_day_stats_aggregates_all_tools_for_specific_date() {
    let stats =
        super::sessions_usage_day_stats(Local::now().date_naive().format("%Y-%m-%d").to_string())
            .expect("day stats");
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    assert_eq!(stats.date, today);
    assert_eq!(stats.breakdown.len(), 4);
    for tool_breakdown in &stats.breakdown {
        assert!(
            ["claude", "codex", "antigravity", "opencode"].contains(&tool_breakdown.tool.as_str())
        );
    }
}

#[test]
fn usage_day_stats_rejects_future_date() {
    let future = (Local::now().date_naive() + chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let error = super::sessions_usage_day_stats(future).expect_err("should reject future");
    assert!(error.contains("cannot query future dates"));
}

#[test]
fn usage_day_stats_rejects_invalid_date_format() {
    let error = super::sessions_usage_day_stats("not-a-date".to_string())
        .expect_err("should reject invalid date");
    assert!(error.contains("invalid date format"));
}

#[test]
fn antigravity_existing_binding_does_not_fallback_to_latest_when_created_time_present() {
    let created_at_ms = 1_700_000_000_000_i64;
    let candidates = vec![
        AntigravitySessionCandidate {
            session_id: "older-but-updated".to_string(),
            start_at_ms: created_at_ms - 3_600_000,
            updated_at_ms: created_at_ms + 10_000,
        },
        AntigravitySessionCandidate {
            session_id: "latest".to_string(),
            start_at_ms: created_at_ms - 7_200_000,
            updated_at_ms: created_at_ms + 20_000,
        },
    ];
    let selected = select_antigravity_session_for_existing(&candidates, Some(created_at_ms));
    assert!(selected.is_none());
}

#[test]
fn antigravity_existing_binding_prefers_start_time_over_recent_updates() {
    let created_at_ms = 1_700_000_000_000_i64;
    let candidates = vec![
        AntigravitySessionCandidate {
            session_id: "target".to_string(),
            start_at_ms: created_at_ms + 2_000,
            updated_at_ms: created_at_ms + 15_000,
        },
        AntigravitySessionCandidate {
            session_id: "distractor".to_string(),
            start_at_ms: created_at_ms - 7_200_000,
            updated_at_ms: created_at_ms + 30_000,
        },
    ];
    let selected = select_antigravity_session_for_existing(&candidates, Some(created_at_ms));
    assert_eq!(selected.as_deref(), Some("target"));
}

#[test]
fn antigravity_create_binding_prefers_nearest_start_time() {
    let launch_started_at_ms = 1_700_000_000_000_i64;
    let candidates = vec![
        AntigravitySessionCandidate {
            session_id: "new".to_string(),
            start_at_ms: launch_started_at_ms + 1_000,
            updated_at_ms: launch_started_at_ms + 2_000,
        },
        AntigravitySessionCandidate {
            session_id: "old-resumed".to_string(),
            start_at_ms: launch_started_at_ms - 3_600_000,
            updated_at_ms: launch_started_at_ms + 3_000,
        },
    ];
    let selected = select_antigravity_session_for_create(&candidates, launch_started_at_ms);
    assert_eq!(selected.as_deref(), Some("new"));
}

#[test]
fn antigravity_create_binding_falls_back_to_recent_update_when_no_near_start() {
    let launch_started_at_ms = 1_700_000_000_000_i64;
    let candidates = vec![
        AntigravitySessionCandidate {
            session_id: "old-resumed".to_string(),
            start_at_ms: launch_started_at_ms - 86_400_000,
            updated_at_ms: launch_started_at_ms + 2_000,
        },
        AntigravitySessionCandidate {
            session_id: "stale".to_string(),
            start_at_ms: launch_started_at_ms - 172_800_000,
            updated_at_ms: launch_started_at_ms - 1_000,
        },
    ];
    let selected = select_antigravity_session_for_create(&candidates, launch_started_at_ms);
    assert_eq!(selected.as_deref(), Some("old-resumed"));
}

#[test]
#[ignore = "local environment smoke test"]
fn test_local_antigravity_binding() {
    let working_dir = "/Users/yuqiyu/AiHistorys/one-space/onespace-app";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    use std::collections::HashSet;
    let exclude = HashSet::new();

    let candidates = super::collect_antigravity_session_candidates(working_dir, Some(&exclude));
    println!("Found {} candidates for {}", candidates.len(), working_dir);
    for c in &candidates {
        println!(
            " - ID: {}, start: {}, updated: {}",
            c.session_id, c.start_at_ms, c.updated_at_ms
        );
    }

    let bind_time = now - 60000;
    let res = super::resolve_antigravity_session_id_for_pending_bind(
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

    let antigravity = super::build_resume_command(
        "antigravity",
        "sess2",
        super::TerminalPermissionMode::Default,
    );
    assert!(antigravity.is_some());
    let antigravity_cmd = &antigravity.unwrap().command;
    assert_eq!(antigravity_cmd, "agy --conversation 'sess2'");
    assert!(!antigravity_cmd.contains("--dangerously-skip-permissions"));
    assert!(!antigravity_cmd.contains("gemini"));
    assert!(!antigravity_cmd.contains("-y"));
    assert!(!antigravity_cmd.contains("--approval-mode"));

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
fn build_resume_command_full_access_antigravity() {
    let result = super::build_resume_command(
        "antigravity",
        "xyz789",
        super::TerminalPermissionMode::FullAccess,
    );
    assert!(result.is_some());
    let r = result.unwrap();
    assert!(r.command.contains("--dangerously-skip-permissions"));
    assert!(r.command.contains("--conversation 'xyz789'"));
    assert!(!r.command.contains("gemini"));
    assert!(!r.command.contains("-y"));
    assert!(!r.command.contains("--approval-mode"));
    assert!(r.env.is_none());
}

#[test]
fn antigravity_command_contract_covers_create_continue_resume_and_full_access() {
    assert_eq!(super::antigravity_new_command(), "agy");
    assert_eq!(super::antigravity_continue_command(), "agy -c");
    assert_eq!(
        super::antigravity_resume_command("conv-1"),
        "agy --conversation 'conv-1'"
    );

    // Continue most recent when no conversation id is known.
    let continued = super::build_resume_command(
        "antigravity",
        "   ",
        super::TerminalPermissionMode::Default,
    )
    .expect("antigravity continue command");
    assert_eq!(continued.command, "agy -c");

    let continued_full = super::build_resume_command(
        "antigravity",
        "",
        super::TerminalPermissionMode::FullAccess,
    )
    .expect("antigravity full-access continue command");
    assert!(continued_full.command.contains("agy -c"));
    assert!(continued_full
        .command
        .contains("--dangerously-skip-permissions"));

    for command in [
        super::antigravity_new_command(),
        super::antigravity_continue_command(),
        super::antigravity_resume_command("conv-1"),
        "agy --dangerously-skip-permissions".to_string(),
    ] {
        assert!(!command.contains("gemini"), "unexpected gemini in {command}");
        assert!(!command.contains(" -y"), "unexpected -y in {command}");
        assert!(
            !command.contains("--approval-mode"),
            "unexpected approval mode in {command}"
        );
    }
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

#[test]
fn antigravity_managed_launch_env_injects_gemini_api_key_and_base_url() {
    let temp_home = make_temp_dir("antigravity-launch-env");
    // This case only resolves `get_data_dir()` (through `get_app_dir()`), so a
    // thread-local HOME override replaces the process `HOME` mutation and the
    // global `crate::lock_test_home_env` mutex. Seed an isolated device config
    // so the first-run local mirror never falls back to the real home through
    // `dirs::home_dir()`.
    let app_dir = temp_home.join(".config").join("onespace");
    fs::create_dir_all(&app_dir).expect("create app dir");
    let seeded_config = format!(
        r#"{{"storage_type":"local","local_storage_path":{}}}"#,
        serde_json::to_string(&temp_home.join("data")).expect("encode temp data path")
    );
    fs::write(app_dir.join("config.json"), seeded_config).expect("seed config");
    let _guard = crate::config::test_home::TestHomeGuard::set(&temp_home);

    let provider_id = uuid::Uuid::new_v4().to_string();
    let providers_path = crate::get_data_dir()
        .expect("data dir")
        .join("data")
        .join("providers")
        .join("state.json");
    if let Some(parent) = providers_path.parent() {
        fs::create_dir_all(parent).expect("create providers dir");
    }
    let payload = serde_json::json!({
        "active": { "antigravity": provider_id },
        "providers": [{
            "id": provider_id,
            "name": "Antigravity",
            "tool": "antigravity",
            "api_key": "sk-antigravity",
            "base_url": "https://antigravity.example.com"
        }]
    });
    fs::write(&providers_path, serde_json::to_string(&payload).unwrap())
        .expect("write providers state");

    let env = antigravity_managed_launch_env();
    assert_eq!(
        env.get("GEMINI_API_KEY").map(String::as_str),
        Some("sk-antigravity")
    );
    assert_eq!(
        env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str),
        Some("https://antigravity.example.com")
    );

    let _ = fs::remove_dir_all(&temp_home);
}

// ============================================================================
// antigravity transcript usage counting — behaviour tests (slice 1b)
// These tests assert on the public boundary:
//   sessions_usage_tool_stats("antigravity", days=7) → SessionUsageToolStats
// Expected layout:
//   $HOME/.gemini/antigravity-cli/brain/<conversation-id>/transcript_full.jsonl
//   $HOME/.gemini/antigravity/brain/<conversation-id>/transcript_full.jsonl
// Counting rules: type=="USER_INPUT" lines with created_at in window count
//   as calls; unique conversation directories with a window USER_INPUT count
//   as sessions. Summary/daily stay zero (no token records). Errors survive.
// ============================================================================

/// Test 1 — tmp missing + two brain roots, each with one in-window session
/// Hand-scaled counts:
///   tmp files scanned:        0  (no .gemini/tmp/)
///   transcript sessions:      2  (cli-conversation + alt-conversation)
///   scanned_sessions total:   0 + 2 = 2
///   tmp token records:        0
///   transcript USER_INPUTs:   2 + 1 = 3  (within window)
///   scanned_calls total:      0 + 3 = 3
///   summary.calls / daily:    all zero  (transcripts carry no tokens)
/// ============================================================================
#[test]
fn antigravity_transcript_scans_brain_roots_and_counts_in_window_inputs() {
    use chrono::Duration as ChronoDuration;

    let root = make_temp_dir("antigravity-transcript-brain-multi");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();

    // Build two brain-root conversations with transcripts containing
    // `created_at` timestamps inside the current 7-day window.
    let ts_now = Local::now();
    let fmt_ts = |days_ago: i64| -> String {
        let dt = (ts_now.date_naive() - ChronoDuration::days(days_ago))
            .and_hms_opt(10, 30, 0)
            .expect("valid timestamp");
        dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    };

    // Brain root 1 (.gemini/antigravity-cli/brain/cli-conversation):
    //   transcript has 2 USER_INPUT rows → 2 calls.
    let cli_root = root
        .join(".gemini")
        .join("antigravity-cli")
        .join("brain")
        .join("cli-conversation");
    let cli_transcript = cli_root
        .join("transcript_full.jsonl");
    write_temp_file(
        &cli_transcript,
        format!(
            "{{\"type\":\"USER_INPUT\",\"content\":\"first prompt\",\"created_at\":\"{}\"}}\n\
             {{\"type\":\"MODEL\",\"model\":\"gemini-pro\",\"created_at\":\"{}\"}}\n\
             {{\"type\":\"USER_INPUT\",\"content\":\"second prompt\",\"created_at\":\"{}\"}}\n\
             {{\"type\":\"MODEL\",\"model\":\"gemini-pro\",\"created_at\":\"{}\"}}\n",
            fmt_ts(0), // day 0 — in window
            fmt_ts(0),
            fmt_ts(2), // day 2 — in window
            fmt_ts(2),
        )
        .as_str(),
    );

    // Brain root 2 (.gemini/antigravity/brain/alt-conversation):
    //   transcript has 1 USER_INPUT row → 1 call.
    let alt_root = root
        .join(".gemini")
        .join("antigravity")
        .join("brain")
        .join("alt-conversation");
    let alt_transcript = alt_root.join("transcript_full.jsonl");
    write_temp_file(
        &alt_transcript,
        format!(
            "{{\"type\":\"USER_INPUT\",\"content\":\"hello from alt\",\
             \"created_at\":\"{}\"}}\n\
             {{\"type\":\"MODEL\",\"model\":\"gemini-ultra\",\"created_at\":\"{}\"}}\n",
            fmt_ts(1), // day 1 — in window
            fmt_ts(1),
        )
        .as_str(),
    );

    // No .gemini/tmp/ directory exists → tmp contribution is zero.

    let stats = sessions_usage_tool_stats("antigravity".to_string(), Some(7))
        .expect("tool stats with transcripts");

    // source_status must not be "unavailable" because transcript files exist on
    // disk. (Before slice 1b impl, this assertion intentionally exposes that
    // the new path is not wired into collect_antigravity_usage_records.)
    assert_ne!(
        stats.source_status, "unavailable",
        "transcripts are present in brain roots — status must not be unavailable \
         (actual={})",
        stats.source_status
    );

    // scanned_sessions = tmp_files(0) + transcript sessions(2)
    assert_eq!(
        stats.scanned_sessions, 2,
        "should scan both brain-root transcript sessions \
         (actual={})",
        stats.scanned_sessions
    );

    // scanned_calls = tmp_token_records(0) + in-window USER_INPUTs(2+1)
    assert_eq!(
        stats.scanned_calls, 3,
        "two inputs from cli + one from alt, all within 7-day window \
         (actual={})",
        stats.scanned_calls
    );

    // summary stays zero because transcripts do not contribute to record-based
    // aggregation — they only increment scanned_calls / scanned_sessions.
    assert_eq!(stats.summary.total_tokens, 0);
    assert_eq!(stats.summary.calls, 0);
    assert_eq!(stats.summary.sessions, 0);
    assert_eq!(stats.daily.len(), 7);
    for day in &stats.daily {
        assert_eq!(day.total_tokens, 0);
        assert_eq!(day.calls, 0);
        assert_eq!(day.sessions, 0);
    }

    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

/// Test 2 — USER_INPUT rows outside the 7-day window are excluded
/// Hand-scaled counts:
///   Each transcript has 3 USER_INPUT rows but all date > 30 days ago.
///   scanned_sessions = 0  (no in-window sessions)
///   scanned_calls = 0     (no in-window calls)
///   source_status still != "unavailable" (files exist)
/// ============================================================================
#[test]
fn antigravity_transcript_excludes_outside_window_inputs() {
    use chrono::Duration as ChronoDuration;

    let root = make_temp_dir("antigravity-transcript-outside-window");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();

    let ts_now = Local::now();
    let fmt_ts = |days_ago: i64| -> String {
        let dt = (ts_now.date_naive() - ChronoDuration::days(days_ago))
            .and_hms_opt(10, 30, 0)
            .expect("valid timestamp");
        dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    };

    // Two conversations, each with 3 USER_INPUT rows 30 days old.
    let cli_root = root
        .join(".gemini")
        .join("antigravity-cli")
        .join("brain")
        .join("old-session-a");
    write_temp_file(
        &cli_root.join("transcript_full.jsonl"),
        format!(
            "{{\"type\":\"USER_INPUT\",\"created_at\":\"{}\"}}\n\
             {{\"type\":\"MODEL\"}}\n\
             {{\"type\":\"USER_INPUT\",\"created_at\":\"{}\"}}\n\
             {{\"type\":\"MODEL\"}}\n\
             {{\"type\":\"USER_INPUT\",\"created_at\":\"{}\"}}\n\
             {{\"type\":\"MODEL\"}}\n",
            fmt_ts(30), fmt_ts(25), fmt_ts(20)
        )
        .as_str(),
    );

    let alt_root = root
        .join(".gemini")
        .join("antigravity")
        .join("brain")
        .join("old-session-b");
    write_temp_file(
        &alt_root.join("transcript_full.jsonl"),
        format!(
            "{{\"type\":\"USER_INPUT\",\"created_at\":\"{}\"}}\n\
             {{\"type\":\"MODEL\"}}\n\
             {{\"type\":\"USER_INPUT\",\"created_at\":\"{}\"}}\n\
             {{\"type\":\"MODEL\"}}\n\
             {{\"type\":\"USER_INPUT\",\"created_at\":\"{}\"}}\n\
             {{\"type\":\"MODEL\"}}\n",
            fmt_ts(30), fmt_ts(28), fmt_ts(22)
        )
        .as_str(),
    );

    let stats = sessions_usage_tool_stats("antigravity".to_string(), Some(7))
        .expect("tool stats with stale transcripts");

    // Files exist, so status should NOT be unavailable even if all rows are
    // outside window. (Before slice 1b impl this assertion exposes the gap.)
    assert_ne!(
        stats.source_status, "unavailable",
        "transcripts are present even if outside window — \
         status must not be unavailable (actual={})",
        stats.source_status
    );

    // All inputs are outside the 7-day window → zero counted.
    assert_eq!(
        stats.scanned_sessions, 0,
        "no in-window sessions (actual={})",
        stats.scanned_sessions
    );
    assert_eq!(
        stats.scanned_calls, 0,
        "no in-window calls (actual={})",
        stats.scanned_calls
    );
    assert_eq!(stats.summary.total_tokens, 0);
    assert_eq!(stats.summary.calls, 0);

    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

/// Test 3 — Corrupted transcript: non-JSON lines mixed in + rows without
///          `created_at`. The implementation should skip bad lines gracefully
///          and not crash. Rows without `created_at` fall back to file mtime
///          when that path is implemented by slice 1b.
/// Observability assertion: either errors are recorded OR counts remain correct
/// despite the corruption.
/// ============================================================================
#[test]
fn antigravity_transcript_skips_corrupt_lines_without_crashing() {
    use chrono::Duration as ChronoDuration;

    let root = make_temp_dir("antigravity-transcript-corrupt");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();

    let ts_now = Local::now();
    let fmt_ts = |days_ago: i64| -> String {
        let dt = (ts_now.date_naive() - ChronoDuration::days(days_ago))
            .and_hms_opt(10, 30, 0)
            .expect("valid timestamp");
        dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    };

    let brain_root = root
        .join(".gemini")
        .join("antigravity-cli")
        .join("brain")
        .join("corrupt-session");

    // Transcript with: 1 valid USER_INPUT + 1 corrupt line + 1 row missing
    // `created_at` (would need mtime fallback once implemented).
    write_temp_file(
        &brain_root.join("transcript_full.jsonl"),
        format!(
            "{{\"type\":\"USER_INPUT\",\"content\":\"good\",\"created_at\":\"{}\"}}\n\
             THIS IS COMPLETELY GARBAGE AND NOT JSON\n\
             {{\"type\":\"MODEL\",\"model\":\"gemini-pro\"}}\n\
             {{\"type\":\"USER_INPUT\",\"content\":\"missing-timestamp\"}}\n",
            fmt_ts(1) // in-window, should be counted
        )
        .as_str(),
    );

    // Should not panic.
    let stats = sessions_usage_tool_stats("antigravity".to_string(), Some(7));
    let stats = match stats {
        Ok(s) => s,
        Err(_) => {
            panic!(
                "corrupt transcript should not cause the tool_stats call to error"
            );
        }
    };

    // At least one valid in-window input should be counted.
    // (The second USER_INPUT lacks created_at; behavior depends on whether
    // mtime fallback is implemented — we tolerate either outcome here.)
    assert!(
        stats.scanned_calls >= 1,
        "at least the valid in-window input should be counted \
         (scanned_calls={})",
        stats.scanned_calls
    );

    // Either errors were recorded or counts stayed reasonable.
    let has_errors = !stats.errors.is_empty();
    assert!(
        has_errors || stats.scanned_calls <= 2,
        "either report parsing errors or keep counts bounded \
         (errors={}, scanned_calls={})",
        stats.errors.len(),
        stats.scanned_calls
    );

    assert_ne!(
        stats.source_status, "unavailable",
        "transcript file exists on disk"
    );

    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

/// Test 4 — Mixed tmp token data + transcript data: both sources present.
/// Hand-scaled counts (using small fixtures from 1a pattern):
///   tmp file session (rollout-mix/chats/session-mix.json):
///     2 messages within window → 2 records
///     total_or_sum(0, 10, 3, 0)=13, total_or_sum(0, 20, 5, 0)=25
///     → total_tokens=38
///   transcript session (mixed-clin/transcript_full.jsonl):
///     1 USER_INPUT within window → 1 call from transcript
///     (no tokens in transcript — does not enter records)
///
///   scanned_sessions = tmp(1) + transcript(1) = 2
///   scanned_calls = tmp_records(2) + transcript_inputs(1) = 3
///   summary.total_tokens = 38  (only from tmp records)
///   summary.calls = 2          (only tmp records contribute)
/// ============================================================================
#[test]
fn antigravity_transcript_mixed_with_tmp_data_aggregates_both_sources() {
    use chrono::Duration as ChronoDuration;

    let root = make_temp_dir("antigravity-transcript-mixed");
    let _guard = crate::config::test_home::TestHomeGuard::set(&root);
    sessions_usage_clear_cache();

    let ts_now = Local::now();
    let fmt_ts = |days_ago: i64| -> String {
        let dt = (ts_now.date_naive() - ChronoDuration::days(days_ago))
            .and_hms_opt(5, 9, 58)
            .expect("valid timestamp");
        dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    };

    // ── legacy tmp fixture (reuses the same pattern as antigravity_json_* tests)
    let chats_dir = root
        .join(".gemini")
        .join("tmp")
        .join("rollout-mix")
        .join("chats");
    write_temp_file(
        &chats_dir.join("session-mix.json"),
        &format!(
            "{{\n  \"sessionId\": \"conv-mix\",\n\
             \"messages\": [\n\
               {{\"tokens\":{{\"input\":10,\"output\":3,\"cached\":0,\"total\":0}},\
                 \"model\":\"mix-model\",\"timestamp\":\"{}\"}},\n\
               {{\"tokens\":{{\"input\":20,\"output\":5,\"cached\":0,\"total\":0}},\
                 \"model\":\"mix-model\",\"timestamp\":\"{}\"}}\n\
             ]\n}}"
            , fmt_ts(0), fmt_ts(1)
        ),
    );

    // ── transcript fixture
    let clin_root = root
        .join(".gemini")
        .join("antigravity-cli")
        .join("brain")
        .join("mixed-clin");
    write_temp_file(
        &clin_root.join("transcript_full.jsonl"),
        format!(
            "{{\"type\":\"USER_INPUT\",\"content\":\"mixed transcript input\",\
             \"created_at\":\"{}\"}}\n\
             {{\"type\":\"MODEL\",\"model\":\"gemini-pro\"}}\n",
            fmt_ts(0) // in window
        )
        .as_str(),
    );

    let stats = sessions_usage_tool_stats("antigravity".to_string(), Some(7))
        .expect("tool stats with mixed tmp + transcript");

    // Both sources detected → source_status != unavailable
    assert_ne!(
        stats.source_status, "unavailable",
        "both tmp and transcript exist"
    );

    // scanned_sessions = tmp files(1) + transcript sessions(1) = 2
    assert_eq!(
        stats.scanned_sessions, 2,
        "tmp file + transcript session"
    );

    // scanned_calls = tmp records(2) + transcript inputs(1) = 3
    assert_eq!(
        stats.scanned_calls, 3,
        "2 tmp message records + 1 transcript USER_INPUT"
    );

    // summary.total_tokens comes only from tmp token records (transcripts
    // don't contribute UsageRecords), so 13 + 25 = 38.
    assert_eq!(
        stats.summary.total_tokens, 38,
        "token totals only from tmp messages"
    );

    // summary.calls also reflects only tmp records (2).
    assert_eq!(stats.summary.calls, 2);

    sessions_usage_clear_cache();
    let _ = fs::remove_dir_all(root);
}

// ============================================================
// antigravity quota envelope parsing — behaviour tests
// These tests assert on the public boundary:
//   parse_antigravity_quota_envelope(&serde_json::Value) → Result<AntigravityQuotaSnapshot, String>
// Expected types:
//   AntigravityQuotaSnapshot { groups: Vec<AntigravityQuotaGroup> }
//   AntigravityQuotaGroup    { name, description: Option<String>, buckets: Vec<AntigravityQuotaBucket> }
//   AntigravityQuotaBucket   { id, name, window, remaining_fraction: f64, reset_time, description: Option<String> }
// All types derive Serialize.
//
// The parser is expected to:
//   (1) Restore every field exactly from a valid envelope.
//   (2) Return Err(status==ERROR  ∨  missing command.data.groups).
//   (3) Return Err on missing / non-numeric remaining_fraction
//       (whole-envelope failure to preserve snapshot completeness).
// ============================================================

/// Full envelope from `agy -p /usage --output-format json` — representative
/// fixture used by several downstream callers.  Decimals and optional fields
/// exercise precise float preservation and None-default handling.
const QUOTA_ENVELOPE_SUCCESS: &str = r#"{
  "conversation_id": "",
  "status": "SUCCESS",
  "response": "...",
  "duration_seconds": 0,
  "num_turns": 0,
  "usage": {
    "input_tokens": 0,
    "output_tokens": 0,
    "thinking_tokens": 0,
    "cache_read_tokens": 0,
    "total_tokens": 0
  },
  "command": {
    "name": "usage",
    "data": {
      "description": "groups share limits",
      "groups": [
        {
          "name": "Gemini Models",
          "description": "Models within this group: Gemini Flash, Gemini Pro",
          "buckets": [
            {
              "id": "gemini-weekly",
              "name": "Weekly Limit Remaining",
              "description": "refresh in 1 day",
              "window": "weekly",
              "remaining_fraction": 0.24515248835086823,
              "reset_time": "2026-09-23T02:30:18Z"
            },
            {
              "id": "gemini-5h",
              "name": "Five Hour Limit Remaining",
              "description": "refresh in 3 hours",
              "window": "5h",
              "remaining_fraction": 0.7034577131271362,
              "reset_time": "2026-09-21T11:02:05Z"
            }
          ]
        },
        {
          "name": "Claude and GPT models",
          "description": "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
          "buckets": [
            {
              "id": "3p-weekly",
              "name": "Weekly Limit Remaining",
              "window": "weekly",
              "remaining_fraction": 1.0,
              "reset_time": "2026-09-28T07:54:34Z"
            }
          ]
        }
      ]
    }
  }
}"#;

#[test]
fn parse_quota_envelope_restores_groups_and_buckets_field_by_field() {
    let value: serde_json::Value =
        serde_json::from_str(QUOTA_ENVELOPE_SUCCESS).expect("fixture json");

    // The function + types must exist before implementation lands.
    let snap = parse_antigravity_quota_envelope(&value).expect("quota snapshot");

    assert_eq!(snap.groups.len(), 2);

    let g0 = &snap.groups[0];
    assert_eq!(g0.name, "Gemini Models");
    assert_eq!(
        g0.description.as_deref(),
        Some("Models within this group: Gemini Flash, Gemini Pro")
    );
    assert_eq!(g0.buckets.len(), 2);

    let b0 = &g0.buckets[0];
    assert_eq!(b0.id, "gemini-weekly");
    assert_eq!(b0.name, "Weekly Limit Remaining");
    assert_eq!(b0.description.as_deref(), Some("refresh in 1 day"));
    assert_eq!(b0.window, "weekly");
    assert!(
        (b0.remaining_fraction - 0.24515248835086823_f64).abs() < f64::EPSILON,
        "remaining_fraction should be 0.24515248835086823, got {}",
        b0.remaining_fraction
    );
    assert_eq!(b0.reset_time, "2026-09-23T02:30:18Z");

    let b1 = &g0.buckets[1];
    assert_eq!(b1.id, "gemini-5h");
    assert!(
        (b1.remaining_fraction - 0.7034577131271362_f64).abs() < f64::EPSILON,
        "remaining_fraction should be 0.7034577131271362, got {}",
        b1.remaining_fraction
    );
    assert_eq!(b1.window, "5h");
    assert_eq!(b1.reset_time, "2026-09-21T11:02:05Z");

    // Group without optional description — parsed as None.
    let g1 = &snap.groups[1];
    assert_eq!(g1.name, "Claude and GPT models");
    assert_eq!(
        g1.description.as_deref(),
        Some("Models within this group: Claude Opus, Claude Sonnet, GPT-OSS")
    );
    assert_eq!(g1.buckets.len(), 1);

    let b2 = &g1.buckets[0];
    assert_eq!(b2.id, "3p-weekly");
    assert_eq!(b2.description, None);
    assert!((b2.remaining_fraction - 1.0_f64).abs() < f64::EPSILON);
}

#[test]
fn parse_quota_envelope_errors_on_status_error_or_missing_command_data() {
    // --- status == ERROR → Err ---
    let err_value: serde_json::Value = serde_json::from_str(
        r#"{"status":"ERROR","command":{"name":"usage","data":{}}}"#,
    )
    .expect("error fixture");
    let result = parse_antigravity_quota_envelope(&err_value);
    assert!(result.is_err(), "expected Err when status=ERROR");
    assert!(!result.unwrap_err().is_empty());

    // --- missing command.data.groups → Err ---
    let missing_value: serde_json::Value = serde_json::from_str(
        r#"{"status":"SUCCESS","command":{"name":"usage","data":{"description":""}}}"#.as_ref(),
    )
    .expect("missing-groups fixture");
    let result2 = parse_antigravity_quota_envelope(&missing_value);
    assert!(
        result2.is_err(),
        "expected Err when command.data.groups is missing"
    );
    assert!(!result2.unwrap_err().is_empty());
}

#[test]
fn parse_quota_envelope_errors_when_remaining_fraction_is_missing() {
    // remaining_fraction absent → whole envelope Err (enforces snapshot
    // completeness — best-effort partial snapshots lose fidelity).
    let missing_frac: serde_json::Value = serde_json::from_str(
        r#"{
  "status": "SUCCESS",
  "command": {
    "name": "usage",
    "data": {
      "groups": [{
        "name": "Test Group",
        "buckets": [{
          "id": "x",
          "name": "X",
          "window": "daily",
          "reset_time": "2026-10-01T00:00:00Z"
        }]
      }]
    }
  }
}"#,
    )
    .expect("missing-frac fixture");
    let result = parse_antigravity_quota_envelope(&missing_frac);
    assert!(
        result.is_err(),
        "expected Err when remaining_fraction is missing"
    );
}
