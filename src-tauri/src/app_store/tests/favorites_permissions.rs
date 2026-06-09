use super::*;
use serde_json::json;

#[test]
fn set_favorite_marks_service_provider_with_timestamp() {
    let mut state = ServiceProvidersState {
        active: HashMap::new(),
        providers: vec![ServiceProviderRecord {
            id: "p1".to_string(),
            name: "Provider 1".to_string(),
            tool: "codex".to_string(),
            api_key: "key".to_string(),
            favorite_at: None,
            ..ServiceProviderRecord::default()
        }],
    };

    let result = set_service_provider_favorite_impl(&mut state, "p1", true);
    assert!(result.is_ok());
    assert!(state.providers[0].favorite_at.is_some());
}

#[test]
fn unset_favorite_clears_service_provider_timestamp() {
    let mut state = ServiceProvidersState {
        active: HashMap::new(),
        providers: vec![ServiceProviderRecord {
            id: "p1".to_string(),
            name: "Provider 1".to_string(),
            tool: "codex".to_string(),
            api_key: "key".to_string(),
            favorite_at: Some(123),
            ..ServiceProviderRecord::default()
        }],
    };

    set_service_provider_favorite_impl(&mut state, "p1", false).unwrap();
    assert_eq!(state.providers[0].favorite_at, None);
}

#[test]
fn refavorite_service_provider_keeps_original_timestamp() {
    let mut state = ServiceProvidersState {
        active: HashMap::new(),
        providers: vec![ServiceProviderRecord {
            id: "p1".to_string(),
            name: "Provider 1".to_string(),
            tool: "codex".to_string(),
            api_key: "key".to_string(),
            favorite_at: Some(456),
            ..ServiceProviderRecord::default()
        }],
    };

    set_service_provider_favorite_impl(&mut state, "p1", true).unwrap();
    assert_eq!(state.providers[0].favorite_at, Some(456));
}

#[test]
fn provider_conversion_chain_preserves_favorite_at() {
    let mut sp = ServiceProviderRecord {
        id: "p1".to_string(),
        name: "Provider 1".to_string(),
        tool: "claude".to_string(),
        api_key: "key".to_string(),
        favorite_at: Some(789),
        ..ServiceProviderRecord::default()
    };
    sp.tool_config
        .insert("remark".to_string(), Value::String("note".to_string()));

    let value = service_provider_to_value(&sp);
    assert_eq!(value.get("favorite_at").and_then(|v| v.as_u64()), Some(789));

    let from_value = service_provider_from_value(value.clone(), None);
    assert_eq!(from_value.favorite_at, Some(789));

    let legacy = service_provider_to_legacy(&sp);
    assert_eq!(
        legacy.get("favorite_at").and_then(|v| v.as_u64()),
        Some(789)
    );

    let provider = service_provider_to_provider_record(&sp);
    assert_eq!(provider.favorite_at, Some(789));

    let input = provider_input_from_value(&legacy).expect("provider input");
    assert_eq!(input.favorite_at, Some(789));

    let restored = provider_from_input(input, None);
    assert_eq!(restored.favorite_at, Some(789));
}

#[test]
fn migrate_providers_to_service_providers_preserves_favorite_at() {
    let old = ProvidersState {
        active: HashMap::new(),
        providers: vec![ProviderRecord {
            core: ProviderCore {
                id: "p1".to_string(),
                name: "Claude".to_string(),
                tool: "claude".to_string(),
                api_key: "key".to_string(),
                code: None,
                base_url: None,
                model: None,
            },
            runtime_policy: ProviderRuntimePolicy::default(),
            favorite_at: Some(321),
            tool_config: Map::new(),
            history: vec![],
            extra: Map::new(),
            is_enabled: None,
            provider_key: None,
        }],
    };

    let migrated = migrate_providers_to_service_providers(old);
    assert_eq!(migrated.providers[0].favorite_at, Some(321));
}

#[test]
fn legacy_export_view_includes_favorite_at() {
    let state = ServiceProvidersState {
        active: HashMap::new(),
        providers: vec![ServiceProviderRecord {
            id: "p1".to_string(),
            name: "Provider 1".to_string(),
            tool: "codex".to_string(),
            api_key: "key".to_string(),
            favorite_at: Some(999),
            ..ServiceProviderRecord::default()
        }],
    };

    let legacy = service_providers_to_legacy_view(&state);
    assert_eq!(
        legacy.providers[0]
            .get("favorite_at")
            .and_then(|v| v.as_u64()),
        Some(999)
    );
}

#[test]
fn set_favorite_unknown_session_returns_error() {
    let mut state = SessionsState::default();
    let result = set_session_favorite_impl(&mut state, "nonexistent", true);
    assert!(result.is_err());
}

#[test]
fn set_favorite_persists_to_disk() {
    with_temp_dir("set-favorite-persists", |_| {
        let _ = config::get_app_dir().unwrap();
        let mut state = SessionsState::default();
        state
            .sessions
            .push(session_record("s1", "claude", "/tmp", 100, "active"));
        state
            .sessions
            .push(session_record("s2", "codex", "/tmp", 200, "active"));
        save_sessions_state(&state).unwrap();

        set_session_favorite_impl(&mut state, "s1", true).unwrap();
        save_sessions_state(&state).unwrap();

        let reloaded = load_sessions_state().unwrap();
        let s1 = reloaded.sessions.iter().find(|s| s.id == "s1").unwrap();
        assert!(s1.favorited_at.is_some());
        let s2 = reloaded.sessions.iter().find(|s| s.id == "s2").unwrap();
        assert!(s2.favorited_at.is_none());
    });
}

#[test]
fn session_to_legacy_includes_favorited_at() {
    let mut state = SessionsState::default();
    state
        .sessions
        .push(session_record("s1", "claude", "/tmp", 100, "active"));

    // Set favorite and check session_to_legacy includes it.
    set_session_favorite_impl(&mut state, "s1", true).unwrap();
    let s1 = state.sessions.iter().find(|s| s.id == "s1").unwrap();
    let json = session_to_legacy(s1);
    assert!(
        json.get("favorited_at").is_some(),
        "favorited_at should be present in session_to_legacy output"
    );
    assert_eq!(json["favorited_at"].as_u64(), s1.favorited_at);

    // Unfavorite and check it's absent.
    set_session_favorite_impl(&mut state, "s1", false).unwrap();
    let s1 = state.sessions.iter().find(|s| s.id == "s1").unwrap();
    let json = session_to_legacy(s1);
    assert!(
        json.get("favorited_at").is_none(),
        "favorited_at should be absent after unfavorite"
    );
}

#[test]
fn filter_and_sort_sessions_with_favorites() {
    let now = now_ts();
    let mut state = SessionsState::default();
    state
        .sessions
        .push(session_record("s1", "claude", "/tmp", now, "active"));
    state
        .sessions
        .push(session_record("s2", "codex", "/tmp", now, "active"));
    set_session_favorite_impl(&mut state, "s1", true).unwrap();

    // Simulate what sessions_list does: filter and sort.
    let filtered = filter_sessions_by_history_window(state.sessions.iter());

    assert_eq!(
        filtered.len(),
        2,
        "both sessions should be in filtered result"
    );

    // First session should be the favorited one (sorted first).
    assert_eq!(filtered[0].id, "s1");
    assert!(filtered[0].favorited_at.is_some());

    let json0 = session_to_legacy(&filtered[0]);
    assert!(
        json0.get("favorited_at").is_some(),
        "favorited session must include favorited_at in JSON"
    );

    // Second session is not favorited.
    assert_eq!(filtered[1].id, "s2");
    let json1 = session_to_legacy(&filtered[1]);
    assert!(
        json1.get("favorited_at").is_none(),
        "non-favorited session must not include favorited_at"
    );
}

// --- Permission mode validation tests ---

#[test]
fn permission_mode_missing_caller_defaults_ok() {
    // When caller does not pass permissionMode, and config is default → ok
    let config = super::ai_sessions::TerminalPermissionMode::Default;
    let result = super::validate_and_resolve_permission_mode(&config, None);
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap(),
        super::ai_sessions::TerminalPermissionMode::Default
    );
}

#[test]
fn permission_mode_config_full_access_requires_confirmation() {
    // Config is full_access, caller passes nothing → PERMISSION_CONFIRMATION_REQUIRED
    let config = super::ai_sessions::TerminalPermissionMode::FullAccess;
    let result = super::validate_and_resolve_permission_mode(&config, None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.code == "PERMISSION_CONFIRMATION_REQUIRED");
}

#[test]
fn permission_mode_full_access_confirmed_ok() {
    // Config full_access, caller confirms full_access → FullAccess
    let config = super::ai_sessions::TerminalPermissionMode::FullAccess;
    let result = super::validate_and_resolve_permission_mode(&config, Some("full_access"));
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap(),
        super::ai_sessions::TerminalPermissionMode::FullAccess
    );
}

#[test]
fn permission_mode_full_access_config_default_override() {
    // Config full_access, caller chooses default → Default
    let config = super::ai_sessions::TerminalPermissionMode::FullAccess;
    let result = super::validate_and_resolve_permission_mode(&config, Some("default"));
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap(),
        super::ai_sessions::TerminalPermissionMode::Default
    );
}

#[test]
fn permission_mode_config_default_rejects_elevation() {
    // Config default, caller tries to elevate to full_access → INVALID_PERMISSION_MODE
    let config = super::ai_sessions::TerminalPermissionMode::Default;
    let result = super::validate_and_resolve_permission_mode(&config, Some("full_access"));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.code == "INVALID_PERMISSION_MODE");
}

#[test]
fn permission_mode_invalid_caller_value_rejected() {
    // Caller passes a bogus value like "yolo" → INVALID_PERMISSION_MODE
    let config = super::ai_sessions::TerminalPermissionMode::FullAccess;
    let result = super::validate_and_resolve_permission_mode(&config, Some("yolo"));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.code == "INVALID_PERMISSION_MODE");
}

#[test]
fn session_provider_id_deserialize_old_json() {
    // Old session JSON without provider_id should deserialize successfully with provider_id = None
    let json = json!({
        "id": "test-session",
        "name": "Test",
        "working_dir": "/tmp",
        "tool": "claude",
        "tool_session_id": "ses_123",
        "created_at": 1000,
        "last_used_at": 1000,
        "status": "active"
    });
    let record: SessionRecord = serde_json::from_value(json).unwrap();
    assert!(record.provider_id.is_none());
}

#[test]
fn session_provider_id_deserialize_new_json() {
    let json = json!({
        "id": "test-session",
        "name": "Test",
        "working_dir": "/tmp",
        "tool": "claude",
        "tool_session_id": "ses_123",
        "created_at": 1000,
        "last_used_at": 1000,
        "status": "active",
        "provider_id": "work-claude"
    });
    let record: SessionRecord = serde_json::from_value(json).unwrap();
    assert_eq!(record.provider_id, Some("work-claude".to_string()));
}

#[test]
fn session_to_legacy_includes_provider_id() {
    let mut record = session_record("s1", "claude", "/tmp", 100, "active");
    record.provider_id = Some("work-claude".to_string());
    let json = session_to_legacy(&record);
    assert_eq!(
        json.get("provider_id").and_then(|v| v.as_str()),
        Some("work-claude")
    );
}

#[test]
fn session_provider_id_none_in_legacy() {
    // session_record already sets provider_id: None
    let record = session_record("s1", "claude", "/tmp", 100, "active");
    let json = session_to_legacy(&record);
    assert_eq!(json.get("provider_id").and_then(|v| v.as_str()), None);
}
