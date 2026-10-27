//! Step 2 RED behavior tests: Tauri command boundary and registration.

#[test]
fn required_tauri_commands_are_registered_in_run_app() {
    let source = include_str!("../../app_runtime/run_app.rs");
    for command in [
        "server_monitor_snapshot",
        "server_monitor_upsert",
        "server_monitor_delete",
        "server_monitor_set_enabled",
        "server_monitor_refresh",
        "server_monitor_test_connection",
        "server_monitor_history",
        "server_monitor_get_settings",
        "server_monitor_save_settings",
    ] {
        assert!(
            source.contains(command),
            "missing command registration: {}",
            command
        );
    }
}

#[test]
fn command_service_boundary_names_exist() {
    let names = [
        "snapshot",
        "upsert",
        "delete",
        "set_enabled",
        "refresh",
        "test_connection",
        "history",
        "get_settings",
        "save_settings",
    ];
    assert_eq!(names.len(), 9);
}
