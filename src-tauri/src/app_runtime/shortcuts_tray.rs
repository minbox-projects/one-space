use super::toggle_main_window;
use crate::ssh_tunnels;
use serde::Serialize;
use std::str::FromStr;
use tauri::menu::{Menu, MenuItem};
use tauri::{Emitter, Manager, WebviewUrl};
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
                toggle_quick_ai_window(app);
            }
        });
    }
    Ok(())
}

pub(super) fn toggle_quick_ai_window(app: &tauri::AppHandle) {
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
pub(super) fn get_tray_label(lang: &str, id: &str) -> &'static str {
    match lang {
        "zh" => match id {
            "show" => "显示窗口",
            "quick" => "快速 AI 会话",
            "search" => "全局搜索",
            "launcher" => "启动台",
            "sessions" => "AI 会话",
            "environments" => "AI 环境",
            "notes" => "笔记",
            "snippets" => "代码片段",
            "settings" => "设置",
            "sync" => "立即同步",
            "quit" => "退出",
            _ => "",
        },
        _ => match id {
            "show" => "Show Window",
            "quick" => "Quick AI Session",
            "search" => "Global Search",
            "launcher" => "Launcher",
            "sessions" => "AI Sessions",
            "environments" => "AI Environments",
            "notes" => "Notes",
            "snippets" => "Snippets",
            "settings" => "Settings",
            "sync" => "Sync Now",
            "quit" => "Quit",
            _ => "",
        },
    }
}

#[derive(Clone, Serialize)]
pub(super) struct TrayActionPayload {
    action: &'static str,
    target: &'static str,
}

pub(super) fn emit_tray_action(app: &tauri::AppHandle, target: &'static str) {
    let payload = TrayActionPayload {
        action: "navigate",
        target,
    };
    let _ = app.emit("tray-action", payload);
}

pub(super) fn create_tray_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    lang: &str,
) -> tauri::Result<Menu<R>> {
    let show_i = MenuItem::with_id(
        app,
        "show",
        get_tray_label(lang, "show"),
        true,
        None::<&str>,
    )?;
    let quick_i = MenuItem::with_id(
        app,
        "quick",
        get_tray_label(lang, "quick"),
        true,
        None::<&str>,
    )?;
    let search_i = MenuItem::with_id(
        app,
        "search",
        get_tray_label(lang, "search"),
        true,
        None::<&str>,
    )?;
    let launcher_i = MenuItem::with_id(
        app,
        "launcher",
        get_tray_label(lang, "launcher"),
        true,
        None::<&str>,
    )?;
    let sessions_i = MenuItem::with_id(
        app,
        "sessions",
        get_tray_label(lang, "sessions"),
        true,
        None::<&str>,
    )?;
    let environments_i = MenuItem::with_id(
        app,
        "environments",
        get_tray_label(lang, "environments"),
        true,
        None::<&str>,
    )?;
    let notes_i = MenuItem::with_id(
        app,
        "notes",
        get_tray_label(lang, "notes"),
        true,
        None::<&str>,
    )?;
    let snippets_i = MenuItem::with_id(
        app,
        "snippets",
        get_tray_label(lang, "snippets"),
        true,
        None::<&str>,
    )?;
    let sync_i = MenuItem::with_id(
        app,
        "sync",
        get_tray_label(lang, "sync"),
        true,
        None::<&str>,
    )?;
    let settings_i = MenuItem::with_id(
        app,
        "settings",
        get_tray_label(lang, "settings"),
        true,
        None::<&str>,
    )?;
    let quit_i = MenuItem::with_id(
        app,
        "quit",
        get_tray_label(lang, "quit"),
        true,
        None::<&str>,
    )?;
    Menu::with_items(
        app,
        &[
            &show_i,
            &quick_i,
            &search_i,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &launcher_i,
            &sessions_i,
            &environments_i,
            &notes_i,
            &snippets_i,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &sync_i,
            &settings_i,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &quit_i,
        ],
    )
}

#[tauri::command]
pub(super) fn quit_app(app: tauri::AppHandle) {
    let _ = ssh_tunnels::shutdown_runtime();
    app.exit(0);
}

#[tauri::command]
pub(super) fn update_tray_menu(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    let menu = create_tray_menu(&app, &lang).map_err(|e| e.to_string())?;
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_menu(Some(menu));
    }
    Ok(())
}
