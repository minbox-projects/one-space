use super::*;

fn history_entry(
    tool: &str,
    tool_session_id: &str,
    title: &str,
    working_dir: &str,
    model_name: Option<&str>,
    created_at_ms: i64,
    updated_at_ms: i64,
) -> ai_sessions::HistorySessionEntry {
    ai_sessions::HistorySessionEntry {
        tool: tool.to_string(),
        tool_session_id: tool_session_id.to_string(),
        title: title.to_string(),
        working_dir: working_dir.to_string(),
        model_name: model_name.map(|value| value.to_string()),
        created_at_ms,
        updated_at_ms,
    }
}

pub(super) fn session_record(
    id: &str,
    tool: &str,
    working_dir: &str,
    created_at: u64,
    status: &str,
) -> SessionRecord {
    SessionRecord {
        id: id.to_string(),
        name: String::new(),
        working_dir: working_dir.to_string(),
        tool: tool.to_string(),
        tool_session_id: String::new(),
        model_name: None,
        name_source: "history".to_string(),
        runtime_mode: "shared".to_string(),
        runtime_profile_id: None,
        preset_id: None,
        created_at,
        last_used_at: created_at,
        status: status.to_string(),
        favorited_at: None,
        provider_id: None,
    }
}

#[test]
fn cli_lookup_prefers_tool_session_id_over_record_id() {
    let mut state = SessionsState::default();

    let mut first = session_record("record-id", "codex", "/tmp/cli-lookup-one", 1, "active");
    first.tool_session_id = "ses_123".to_string();
    state.sessions.push(first);

    let mut second = session_record("ses_123", "claude", "/tmp/cli-lookup-two", 2, "active");
    second.tool_session_id = "claude_456".to_string();
    state.sessions.push(second);

    let matched = find_cli_session_in_state(&state, "ses_123").expect("session should match");

    assert_eq!(matched.tool, "codex");
    assert_eq!(matched.tool_session_id, "ses_123");
    assert_eq!(matched.working_dir, "/tmp/cli-lookup-one");
    assert_eq!(matched.id, "record-id");
}

#[test]
fn cli_lookup_falls_back_to_record_id() {
    let mut state = SessionsState::default();
    let mut session = session_record("history-codex-1", "codex", "/tmp/cli-lookup", 1, "active");
    session.tool_session_id = "ses_999".to_string();
    state.sessions.push(session);

    let matched =
        find_cli_session_in_state(&state, "history-codex-1").expect("session should match");

    assert_eq!(matched.tool, "codex");
    assert_eq!(matched.tool_session_id, "ses_999");
    assert_eq!(matched.id, "history-codex-1");
}

#[test]
fn create_flow_marks_session_active_when_launch_returns_real_session_id() {
    let working_dir = normalize_session_working_dir("/tmp/opencode-create-active");
    let mut session = session_record(
        "created",
        "opencode",
        &working_dir,
        1_700_000_000,
        "pending_bind",
    );

    apply_resolved_session_id_after_create(&mut session, Some(" ses_123 "), 1_700_000_111);

    assert_eq!(session.tool_session_id, "ses_123");
    assert_eq!(session.status, "active");
    assert_eq!(session.last_used_at, 1_700_000_111);
}

#[test]
fn create_flow_keeps_session_pending_bind_when_launch_returns_no_session_id() {
    let working_dir = normalize_session_working_dir("/tmp/opencode-create-pending");
    let mut session = session_record("created", "opencode", &working_dir, 1_700_000_000, "active");
    session.tool_session_id = "stale-session-id".to_string();

    apply_resolved_session_id_after_create(&mut session, None, 1_700_000_222);

    assert!(session.tool_session_id.is_empty());
    assert_eq!(session.status, "pending_bind");
    assert_eq!(session.last_used_at, 1_700_000_222);
}

#[test]
fn history_sync_binds_placeholder_session() {
    let working_dir = normalize_session_working_dir("/tmp/history-bind");
    let mut state = SessionsState {
        sessions: vec![session_record(
            "placeholder",
            "codex",
            &working_dir,
            1_700_000_000,
            "pending_bind",
        )],
        ..SessionsState::default()
    };

    let outcome = apply_history_entries_to_sessions_state(
        &mut state,
        "codex",
        vec![history_entry(
            "codex",
            "codex-session-1",
            "Imported Codex Title",
            &working_dir,
            Some("gpt-5.4"),
            1_700_000_001_000,
            1_700_000_005_000,
        )],
        1_700_000_010,
    );

    assert!(outcome.list_changed);
    assert_eq!(state.sessions.len(), 1);
    let session = &state.sessions[0];
    assert_eq!(session.tool_session_id, "codex-session-1");
    assert_eq!(session.name, "Imported Codex Title");
    assert_eq!(session.model_name.as_deref(), Some("gpt-5.4"));
    assert_eq!(session.status, "active");
}

#[test]
fn history_sync_preserves_manual_name_but_updates_model() {
    let working_dir = normalize_session_working_dir("/tmp/history-manual");
    let mut session = session_record("existing", "claude", &working_dir, 1_700_000_000, "active");
    session.name = "Manual Title".to_string();
    session.name_source = "manual".to_string();
    session.tool_session_id = "claude-session-1".to_string();
    let mut state = SessionsState {
        sessions: vec![session],
        ..SessionsState::default()
    };

    let outcome = apply_history_entries_to_sessions_state(
        &mut state,
        "claude",
        vec![history_entry(
            "claude",
            "claude-session-1",
            "History Title",
            &working_dir,
            Some("qwen3.5-plus"),
            1_700_000_000_000,
            1_700_000_009_000,
        )],
        1_700_000_010,
    );

    assert!(outcome.list_changed);
    let session = &state.sessions[0];
    assert_eq!(session.name, "Manual Title");
    assert_eq!(session.model_name.as_deref(), Some("qwen3.5-plus"));
}

#[test]
fn history_sync_preserves_existing_favorite_timestamp() {
    let working_dir = normalize_session_working_dir("/tmp/history-favorite");
    let mut session = session_record("existing", "codex", &working_dir, 1_700_000_000, "active");
    session.tool_session_id = "codex-session-1".to_string();
    session.favorited_at = Some(1_700_000_123);
    let mut state = SessionsState {
        sessions: vec![session],
        ..SessionsState::default()
    };

    let outcome = apply_history_entries_to_sessions_state(
        &mut state,
        "codex",
        vec![history_entry(
            "codex",
            "codex-session-1",
            "Updated Title",
            &working_dir,
            Some("gpt-5.5"),
            1_700_000_001_000,
            1_700_000_009_000,
        )],
        1_700_000_010,
    );

    assert!(outcome.list_changed);
    let session = &state.sessions[0];
    assert_eq!(session.favorited_at, Some(1_700_000_123));
    assert_eq!(session.name, "Updated Title");
}

#[test]
fn history_sync_skips_tombstoned_sessions() {
    let working_dir = normalize_session_working_dir("/tmp/history-tombstone");
    let mut state = SessionsState::default();
    state
        .tombstones
        .insert(history_tombstone_key("gemini", "gemini-session-1").expect("tombstone key"));

    let outcome = apply_history_entries_to_sessions_state(
        &mut state,
        "gemini",
        vec![history_entry(
            "gemini",
            "gemini-session-1",
            "Should Stay Hidden",
            &working_dir,
            Some("gemini-3-pro-preview"),
            1_700_000_000_000,
            1_700_000_001_000,
        )],
        1_700_000_010,
    );

    assert!(!outcome.list_changed);
    assert!(state.sessions.is_empty());
}

#[test]
fn normalize_sessions_state_marks_existing_backfills_with_baseline_parser_version() {
    let mut state = SessionsState::default();
    let codex = state
        .history_sync
        .tools
        .entry("codex".to_string())
        .or_insert_with(SessionsHistoryToolState::default);
    codex.full_backfill_done = true;
    codex.parser_version = 0;

    let changed = normalize_sessions_state(&mut state);

    assert!(changed);
    assert_eq!(
        state
            .history_sync
            .tools
            .get("codex")
            .map(|tool| tool.parser_version),
        Some(HISTORY_SYNC_BASE_PARSER_VERSION)
    );
}

#[test]
fn history_sync_requires_full_backfill_when_codex_parser_version_is_stale() {
    let tool_state = SessionsHistoryToolState {
        full_backfill_done: true,
        parser_version: HISTORY_SYNC_BASE_PARSER_VERSION,
        last_seen_updated_at_ms: 1,
        last_completed_at: Some(1),
    };

    assert!(history_sync_requires_full_backfill(
        "codex",
        Some(&tool_state)
    ));
    assert!(!history_sync_requires_full_backfill(
        "claude",
        Some(&tool_state)
    ));
}

#[test]
fn history_sync_requires_full_backfill_when_opencode_parser_version_is_stale() {
    let tool_state = SessionsHistoryToolState {
        full_backfill_done: true,
        parser_version: HISTORY_SYNC_BASE_PARSER_VERSION,
        last_seen_updated_at_ms: 1,
        last_completed_at: Some(1),
    };

    assert!(history_sync_requires_full_backfill(
        "opencode",
        Some(&tool_state)
    ));
    assert!(!history_sync_requires_full_backfill(
        "gemini",
        Some(&tool_state)
    ));
}

#[test]
fn sort_sessions_favorited_first() {
    let mut sessions = vec![
        session_record("a", "claude", "/tmp", 100, "active"),
        session_record("b", "claude", "/tmp", 200, "active"),
    ];
    sessions[0].favorited_at = Some(150);

    sort_sessions_for_display(&mut sessions);
    assert_eq!(sessions[0].id, "a");
    assert_eq!(sessions[1].id, "b");
}

#[test]
fn sort_sessions_multiple_favorites_by_favorited_at_desc() {
    let mut sessions = vec![
        session_record("a", "claude", "/tmp", 100, "active"),
        session_record("b", "claude", "/tmp", 200, "active"),
        session_record("c", "claude", "/tmp", 300, "active"),
    ];
    sessions[0].favorited_at = Some(150);
    sessions[2].favorited_at = Some(350);

    sort_sessions_for_display(&mut sessions);
    assert_eq!(sessions[0].id, "c");
    assert_eq!(sessions[1].id, "a");
    assert_eq!(sessions[2].id, "b");
}

#[test]
fn sort_sessions_non_favoritized_by_last_used_desc() {
    let mut sessions = vec![
        session_record("a", "claude", "/tmp", 100, "active"),
        session_record("b", "claude", "/tmp", 200, "active"),
    ];

    sort_sessions_for_display(&mut sessions);
    assert_eq!(sessions[0].id, "b");
    assert_eq!(sessions[1].id, "a");
}

#[test]
fn favorited_sessions_survive_history_window_filter() {
    let cutoff = session_history_cutoff_ts();
    let old_ts = cutoff.saturating_sub(86400 * 30); // 30 days ago
    let mut sessions = vec![session_record("old", "claude", "/tmp", old_ts, "active")];
    sessions[0].favorited_at = Some(old_ts);
    sessions[0].last_used_at = old_ts;

    let filtered = filter_sessions_by_history_window(sessions.iter());
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "old");
}

#[test]
fn set_favorite_marks_session_with_timestamp() {
    let mut state = SessionsState::default();
    state
        .sessions
        .push(session_record("s1", "claude", "/tmp", 100, "active"));
    assert!(state.sessions[0].favorited_at.is_none());

    let result = set_session_favorite_impl(&mut state, "s1", true);
    assert!(result.is_ok());
    assert!(state.sessions[0].favorited_at.is_some());
    let ts = state.sessions[0].favorited_at.unwrap();
    assert!(ts >= 100);
}

#[test]
fn unfavorite_clears_timestamp() {
    let mut state = SessionsState::default();
    state
        .sessions
        .push(session_record("s1", "claude", "/tmp", 100, "active"));
    state.sessions[0].favorited_at = Some(500);

    set_session_favorite_impl(&mut state, "s1", false).unwrap();
    assert!(state.sessions[0].favorited_at.is_none());
}

#[test]
fn refavorite_keeps_original_timestamp() {
    let mut state = SessionsState::default();
    state
        .sessions
        .push(session_record("s1", "claude", "/tmp", 100, "active"));
    let first_ts = 500u64;
    state.sessions[0].favorited_at = Some(first_ts);

    set_session_favorite_impl(&mut state, "s1", true).unwrap();
    assert_eq!(state.sessions[0].favorited_at, Some(first_ts));
}
