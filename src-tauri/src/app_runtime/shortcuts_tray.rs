use super::toggle_main_window;
use std::str::FromStr;
use tauri::{Manager, WebviewUrl};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[tauri::command]
pub(super) fn resize_window(window: tauri::Window, height: f64) -> Result<(), String> {
    window
        .set_size(tauri::LogicalSize::new(600.0, height))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(super) fn check_cli_installed() -> bool {
    let home_dir = match dirs::home_dir() {
        Some(path) => path,
        None => return false,
    };
    home_dir
        .join(".local")
        .join("bin")
        .join("onespace")
        .exists()
}

#[cfg(test)]
mod tests {
    use std::net::{SocketAddr, TcpStream};
    use std::time::{Duration, Instant};

    use crate::app_runtime::cli::build_cli_script_content;

    fn webhook_listener_accepts_connections(port: u16) -> bool {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok()
    }

    fn wait_for_webhook_listener_closed(port: u16, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if !webhook_listener_accepts_connections(port) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    struct WebhookServerGuard(
        std::sync::Arc<crate::webhook_catcher::server::WebhookServerManager>,
    );

    impl Drop for WebhookServerGuard {
        fn drop(&mut self) {
            let _ = self.0.stop();
        }
    }

    #[test]
    fn shutdown_runtime_services_closes_running_webhook_listener_idempotently() {
        let _app = tauri::test::mock_app();
        let manager = crate::webhook_catcher::server::WebhookServerManager::global();
        let _ = manager.stop();

        let port = tauri::async_runtime::block_on(manager.start(Some(0)))
            .expect("webhook listener should start on an ephemeral loopback port");
        // Stops the listener on any assertion failure, including the precondition.
        let _guard = WebhookServerGuard(manager.clone());

        assert!(
            webhook_listener_accepts_connections(port),
            "webhook listener should be reachable before shutdown"
        );

        super::shutdown_runtime_services();
        let closed_after_shutdown = wait_for_webhook_listener_closed(port, Duration::from_secs(2));

        assert!(
            closed_after_shutdown,
            "running webhook listener on port {port} stayed reachable after shutdown_runtime_services"
        );
        assert!(
            !webhook_listener_accepts_connections(port),
            "webhook listener should stay closed after shutdown"
        );

        super::shutdown_runtime_services();
        super::shutdown_runtime_services();
        assert!(
            !webhook_listener_accepts_connections(port),
            "repeated shutdown must not reopen the webhook listener"
        );
    }

    #[test]
    fn cli_script_preserves_claude_config_dir_stderr() {
        let script = build_cli_script_content("/tmp/sessions.json", "/tmp/onespace");
        assert!(script.contains(
            r#"CONFIG_DIR=$("$APP_BIN" __onespace_cli_get_claude_config_dir "$PROFILE_ID")"#
        ));
        assert!(
            !script.contains(r#"__onespace_cli_get_claude_config_dir "$PROFILE_ID" 2>/dev/null"#)
        );
    }

    #[test]
    fn cli_script_only_prints_profile_not_found_for_empty_success_output() {
        let script = build_cli_script_content("/tmp/sessions.json", "/tmp/onespace");
        assert!(script.contains("if [ $STATUS -eq 0 ]; then"));
        assert!(script.contains(r#"echo "Claude profile not found: $PROFILE_ID" >&2"#));
    }

    #[test]
    fn fallback_tray_labels_cover_both_languages_and_unknown_ids() {
        assert_eq!(
            super::get_fallback_tray_label("zh", "toggle"),
            "显示/隐藏 OneSpace"
        );
        assert_eq!(
            super::get_fallback_tray_label("zh", "quit"),
            "退出 OneSpace"
        );
        assert_eq!(
            super::get_fallback_tray_label("en", "toggle"),
            "Show/Hide OneSpace"
        );
        assert_eq!(
            super::get_fallback_tray_label("en", "quit"),
            "Quit OneSpace"
        );
        // "any other language" falls back to the English bootstrap labels.
        assert_eq!(
            super::get_fallback_tray_label("fr", "toggle"),
            "Show/Hide OneSpace"
        );
        assert_eq!(
            super::get_fallback_tray_label("fr", "quit"),
            "Quit OneSpace"
        );
        assert_eq!(super::get_fallback_tray_label("zh", "unknown-id"), "");
        assert_eq!(super::get_fallback_tray_label("en", "unknown-id"), "");
    }
}

#[tauri::command]
pub(super) fn update_shortcuts(
    app: tauri::AppHandle,
    main: String,
    quick: String,
) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Ok(s) = Shortcut::from_str(&main) {
        let _ = gs.on_shortcut(s, move |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_main_window(app.clone());
            }
        });
    }
    if let Ok(s) = Shortcut::from_str(&quick) {
        let _ = gs.on_shortcut(s, move |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_quick_ai_window_internal(&app);
            }
        });
    }
    Ok(())
}

#[tauri::command]
pub(super) fn toggle_quick_ai_window(app: tauri::AppHandle) {
    toggle_quick_ai_window_internal(&app);
}

pub(super) fn toggle_quick_ai_window_internal(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("quick-ai") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
            let w = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(120));
                let _ = w.set_focus();
            });
        }
    } else {
        if let Ok(window) = tauri::WebviewWindowBuilder::new(
            app,
            "quick-ai",
            WebviewUrl::App("index.html?view=quick-ai".into()),
        )
        .title("Quick AI")
        .inner_size(600.0, 70.0)
        .resizable(true)
        .decorations(false)
        .always_on_top(true)
        .center()
        .transparent(true)
        .skip_taskbar(true)
        .build()
        {
            let _ = window.set_focus();
            let w = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(180));
                let _ = w.set_focus();
            });
        }
    }
}

use tauri_plugin_global_shortcut::ShortcutState;

pub(super) fn get_fallback_tray_label(lang: &str, id: &str) -> &'static str {
    match lang {
        "zh" => match id {
            "toggle" => "显示/隐藏 OneSpace",
            "quit" => "退出 OneSpace",
            _ => "",
        },
        _ => match id {
            "toggle" => "Show/Hide OneSpace",
            "quit" => "Quit OneSpace",
            _ => "",
        },
    }
}

pub(super) fn shutdown_runtime_services() {
    crate::file_sharing::request_shutdown();
    let _ = crate::ssh_tunnels::shutdown_runtime();
    // Stop the process-owned background services too: the gateway listener
    // (signal + clear, idempotent) and the template auto-refresh schedule.
    crate::ai_gateway::stop_gateway_for_shutdown();
    crate::ai_gateway::stop_template_auto_refresh_scheduler();
    crate::ai_gateway::stop_daily_reset_scanner();
    // Stop and release the BLE lock runtime; idempotent (REQ-011/AC-014).
    super::run_app::ble_lock_native::shutdown_runtime();
    // Stop and kill all port tunnel processes on app shutdown.
    crate::tunnel::shutdown_all();
    // Stop the process-owned SSL certificate scheduler.
    crate::ssl_certificate::stop_ssl_scheduler();
    // Stop the process-owned server monitor scheduler and close owned sessions.
    crate::server_monitor::runtime::shutdown_global();
    // Stop the process-owned webhook listener; `stop` is idempotent.
    let _ = crate::webhook_catcher::server::WebhookServerManager::global().stop();
}

#[tauri::command]
pub(super) fn quit_app(app: tauri::AppHandle) {
    shutdown_runtime_services();
    app.exit(0);
}

