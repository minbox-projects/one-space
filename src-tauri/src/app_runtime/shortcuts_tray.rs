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
    use crate::app_runtime::cli::build_cli_script_content;

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

pub(super) fn toggle_quick_assistant_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("quick-assistant") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.show();
            let _ = window.set_focus();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    } else if let Ok(window) = tauri::WebviewWindowBuilder::new(
        app,
        "quick-assistant",
        WebviewUrl::App("index.html?view=quick-assistant".into()),
    )
    .title("Quick Assistant")
    .inner_size(760.0, 560.0)
    .min_inner_size(540.0, 420.0)
    .resizable(true)
    .decorations(false)
    .always_on_top(true)
    .center()
    .transparent(false)
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

pub(super) fn toggle_selection_assistant_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("selection-assistant") {
        let _ = window.show();
        let _ = window.set_focus();
    } else if let Ok(window) = tauri::WebviewWindowBuilder::new(
        app,
        "selection-assistant",
        WebviewUrl::App("index.html?view=selection-assistant".into()),
    )
    .title("Selection Assistant")
    .inner_size(760.0, 560.0)
    .min_inner_size(540.0, 420.0)
    .resizable(true)
    .decorations(false)
    .always_on_top(true)
    .center()
    .transparent(false)
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
}

#[tauri::command]
pub(super) fn quit_app(app: tauri::AppHandle) {
    shutdown_runtime_services();
    app.exit(0);
}

