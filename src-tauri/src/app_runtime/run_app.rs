use crate::{
    ai_assistant, ai_env, ai_news, ai_request_capture, ai_sessions, app_store, assistant_mcp,
    backup, cli_updates, config, config_conflict, file_sharing, mcp_export, mcp_servers,
    mcp_templates, messages, protocol_router, proxy, secrets, skills, ssh_tunnels, storage,
    subagents, version_detect, workflows, workspaces,
};
use std::str::FromStr;
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use super::{
    cli, create_tray_menu, emit_tray_action, handle_internal_cli_command, oauth_open,
    runtime_services, setup_proxy_monitor, setup_sessions_history_sync_service, shortcuts_tray,
    ssh_oauth, toggle_main_window, toggle_quick_ai_window, windows_data,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if handle_internal_cli_command() {
        return;
    }
    tauri::Builder::default()
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                #[cfg(target_os = "macos")]
                if window.label() == "main" {
                    let _ = window
                        .app_handle()
                        .set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);
            let cfg = config::get_config().unwrap_or_default();
            let lang = cfg.language.unwrap_or_else(|| "zh".to_string());
            let menu = create_tray_menu(app.handle(), &lang)?;
            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        windows_data::show_main_window(app.clone());
                    }
                    "quick" => {
                        toggle_quick_ai_window(app);
                    }
                    "search" => {
                        windows_data::show_main_window(app.clone());
                        emit_tray_action(app, "omni-search");
                    }
                    "launcher" => {
                        windows_data::show_main_window(app.clone());
                        emit_tray_action(app, "launcher");
                    }
                    "sessions" => {
                        windows_data::show_main_window(app.clone());
                        emit_tray_action(app, "ai-sessions");
                    }
                    "environments" => {
                        windows_data::show_main_window(app.clone());
                        emit_tray_action(app, "ai-environments");
                    }
                    "notes" => {
                        windows_data::show_main_window(app.clone());
                        emit_tray_action(app, "notes");
                    }
                    "snippets" => {
                        windows_data::show_main_window(app.clone());
                        emit_tray_action(app, "snippets");
                    }
                    "sync" => {
                        let _ = app.emit("trigger-sync", ());
                    }
                    "settings" => {
                        windows_data::show_main_window(app.clone());
                        emit_tray_action(app, "settings");
                    }
                    "quit" => {
                        ai_request_capture::request_shutdown();
                        file_sharing::request_shutdown();
                        let _ = ssh_tunnels::shutdown_runtime();
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            let main_s = cfg.main_shortcut.unwrap_or_else(|| "Alt+Space".to_string());
            let quick_s = cfg
                .quick_ai_shortcut
                .unwrap_or_else(|| "Alt+Shift+A".to_string());
            let gs = app.global_shortcut();
            if let Ok(s) = Shortcut::from_str(&main_s) {
                let _ = gs.on_shortcut(s, move |app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_main_window(app.clone());
                    }
                });
            }
            if let Ok(s) = Shortcut::from_str(&quick_s) {
                let _ = gs.on_shortcut(s, move |app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_quick_ai_window(app);
                    }
                });
            }

            crate::proxy::init_proxy_manager();
            setup_proxy_monitor(app.handle());
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = protocol_router::protocol_router_autostart().await;
                let _ = app_handle.emit("protocol-router-status-update", ());
            });
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let status = ai_request_capture::ai_request_capture_autostart().await;
                let _ = app_handle.emit("ai-request-capture-status-update", status);
            });
            setup_sessions_history_sync_service(app.handle());
            crate::ai_assistant::init_scheduler(app.handle().clone());
            ssh_tunnels::start_system_wake_observer(app.handle().clone());
            ssh_tunnels::start_sleep_resume_monitor(app.handle().clone());
            // Avoid running heavy migration work before first-run onboarding.
            // Otherwise startup may create default data and suppress onboarding.
            let should_show_onboarding = config::should_show_onboarding().unwrap_or(false);
            if !should_show_onboarding {
                let _ = app_store::ensure_migrated_on_startup();
                let _ = workflows::workflows_cleanup_runtime_profiles_on_startup();
                workspaces::schedule_sync_from_sessions(app.handle().clone());
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = ssh_tunnels::ssh_tunnels_bootstrap(app_handle).await;
                });
            }
            std::thread::spawn(|| loop {
                std::thread::sleep(std::time::Duration::from_secs(30 * 60));
                let _ = workflows::workflows_cleanup_runtime_profiles_on_startup();
            });
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::skills::skills_rescan_mirror(app_handle.clone()).await;
                let _ = crate::skills::skills_reconcile(app_handle.clone(), None, None, None).await;
                let _ = crate::subagents::subagents_rescan_mirror(app_handle.clone()).await;
                let _ = crate::subagents::subagents_reconcile(app_handle, None, None, None).await;
            });

            Ok(())
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_oauth::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            cli::install_cli,
            ssh_oauth::get_ssh_hosts,
            ssh_oauth::connect_ssh,
            ssh_oauth::connect_ssh_custom,
            ssh_tunnels::ssh_tunnel_groups_list,
            ssh_tunnels::ssh_tunnel_group_upsert,
            ssh_tunnels::ssh_tunnel_group_delete,
            ssh_tunnels::ssh_tunnels_list,
            ssh_tunnels::ssh_tunnel_upsert,
            ssh_tunnels::ssh_tunnel_delete,
            ssh_tunnels::ssh_tunnel_connect,
            ssh_tunnels::ssh_tunnel_disconnect,
            ssh_tunnels::ssh_tunnel_group_connect,
            ssh_tunnels::ssh_tunnel_group_disconnect,
            ssh_tunnels::ssh_tunnel_probe_draft,
            ssh_tunnels::ssh_tunnel_probe_saved,
            ssh_tunnels::ssh_tunnels_refresh_status,
            ssh_tunnels::ssh_tunnels_snapshot,
            storage::read_snippets,
            storage::save_snippets,
            storage::read_bookmarks,
            storage::save_bookmarks,
            oauth_open::open_local_path,
            oauth_open::open_external_url,
            storage::read_notes,
            storage::save_notes,
            storage::read_game_data,
            storage::save_game_data,
            ai_news::ai_news_read,
            ai_news::ai_news_sync_now,
            ai_news::ai_news_sync_status_get,
            shortcuts_tray::quit_app,
            ssh_oauth::exchange_google_token,
            ssh_oauth::refresh_google_token,
            oauth_open::start_google_oauth,
            config::get_storage_config,
            config::save_storage_config,
            config::save_shared_profile,
            config::should_show_onboarding,
            messages::messages_list,
            messages::messages_unread_count,
            messages::messages_create,
            messages::messages_mark_read,
            messages::messages_mark_all_read,
            ai_env::get_master_password,
            ai_env::change_master_password,
            ai_env::skip_claude_onboarding_login,
            ai_assistant::ai_workspace_bootstrap,
            ai_assistant::workspace_settings_get,
            ai_assistant::workspace_settings_save,
            ai_assistant::workspace_model_roles_get,
            ai_assistant::workspace_model_roles_save,
            ai_assistant::provider_connection_test,
            ai_assistant::provider_models_fetch,
            ai_assistant::workspace_assistants_list,
            ai_assistant::workspace_assistant_upsert,
            ai_assistant::workspace_assistant_delete,
            ai_assistant::workspace_assistant_test_run,
            assistant_mcp::workspace_assistant_mcp_catalog,
            assistant_mcp::mcp_tool_preview_refresh,
            ai_assistant::workspace_conversations_list,
            ai_assistant::workspace_conversation_get,
            ai_assistant::workspace_conversation_create,
            ai_assistant::workspace_conversation_update,
            ai_assistant::workspace_conversation_delete,
            ai_assistant::workspace_conversation_reset_context,
            ai_assistant::workspace_schedule_resolve_draft,
            ai_assistant::workspace_conversation_send,
            ai_assistant::workspace_automations_list,
            ai_assistant::workspace_automation_upsert,
            ai_assistant::workspace_automation_delete,
            ai_assistant::workspace_automation_toggle,
            ai_assistant::workspace_automation_run_now,
            ai_assistant::workspace_quick_assistant_get,
            ai_assistant::workspace_quick_assistant_save,
            ai_assistant::workspace_selection_assistant_get,
            ai_assistant::workspace_selection_assistant_save,
            secrets::get_secret,
            secrets::save_secret,
            secrets::delete_secret,
            shortcuts_tray::update_shortcuts,
            shortcuts_tray::update_tray_menu,
            windows_data::hide_window,
            windows_data::hide_quick_ai_window,
            windows_data::show_quick_assistant_window,
            windows_data::hide_quick_assistant_window,
            windows_data::show_selection_assistant_window,
            windows_data::hide_selection_assistant_window,
            shortcuts_tray::resize_window,
            windows_data::show_main_window,
            shortcuts_tray::check_cli_installed,
            // MCP Servers
            mcp_servers::get_mcp_servers,
            mcp_servers::save_mcp_server,
            mcp_servers::delete_mcp_server,
            mcp_servers::link_mcp_to_providers,
            mcp_servers::get_mcp_model_switch_states,
            mcp_servers::refresh_mcp_local_install_state,
            mcp_servers::set_mcp_model_switch,
            mcp_servers::mcp_updates_check_background,
            mcp_servers::mcp_updates_status_get,
            mcp_servers::mcp_update_apply,
            mcp_servers::debug_decrypt_all,
            // MCP Templates
            mcp_templates::list_mcp_templates,
            mcp_templates::get_mcp_template,
            // Backup
            backup::create_backup,
            backup::list_backups,
            backup::restore_backup,
            backup::cleanup_old_backups,
            backup::delete_backup,
            // MCP Export/Import
            mcp_export::export_mcp_config,
            mcp_export::import_mcp_config,
            // Version Detection
            version_detect::detect_cli_version,
            version_detect::check_config_compatibility,
            version_detect::get_all_config_compatibility,
            // CLI Updates
            cli_updates::check_cli_update,
            cli_updates::apply_cli_update,
            // Config Conflict
            config_conflict::check_config_conflicts,
            config_conflict::apply_ai_environment_force,
            // Proxy
            proxy::get_proxy_config,
            proxy::save_proxy_config,
            proxy::test_proxy_connection,
            runtime_services::proxy_http_request,
            // Protocol router
            protocol_router::protocol_router_get_config,
            protocol_router::protocol_router_save_config,
            protocol_router::protocol_router_start,
            protocol_router::protocol_router_stop,
            protocol_router::protocol_router_status,
            protocol_router::protocol_router_rotate_token,
            protocol_router::protocol_router_base_url_for_claude_provider,
            protocol_router::protocol_router_test_connection,
            protocol_router::protocol_router_stats,
            // AI request capture storage and lifecycle
            ai_request_capture::ai_request_capture_get_config,
            ai_request_capture::ai_request_capture_save_config,
            ai_request_capture::ai_request_capture_start,
            ai_request_capture::ai_request_capture_stop,
            ai_request_capture::ai_request_capture_status,
            ai_request_capture::ai_request_capture_list,
            ai_request_capture::ai_request_capture_get,
            ai_request_capture::ai_request_capture_clear,
            ai_request_capture::ai_request_capture_export_har,
            ai_request_capture::ai_request_capture_generate_curl,
            // Temporary LAN file sharing
            file_sharing::file_sharing_networks,
            file_sharing::file_sharing_start,
            file_sharing::file_sharing_status,
            file_sharing::file_sharing_stop,
            // New service_providers domain (replaces providers_*)
            app_store::service_providers_list,
            app_store::service_providers_upsert,
            app_store::service_providers_delete,
            app_store::service_providers_set_active,
            app_store::service_providers_set_inactive,
            app_store::service_providers_set_favorite,
            app_store::service_providers_set_env_managed,
            app_store::service_providers_export,
            app_store::service_providers_import_preview,
            app_store::service_providers_import_apply,
            app_store::service_providers_list_synced_other_devices,
            app_store::service_providers_auto_import_from_system,
            app_store::service_provider_presets_list,
            app_store::service_provider_presets_upsert,
            app_store::service_provider_presets_delete,
            ai_env::service_provider_fetch_models,
            // New storage/domain/projection/sync/migration API
            app_store::storage_get_snapshot,
            app_store::dashboard_counts,
            app_store::cli_env_probe,
            app_store::launcher_list,
            app_store::launcher_upsert,
            app_store::launcher_delete,
            app_store::launcher_reorder,
            app_store::launcher_mark_launched,
            app_store::launcher_set_trust,
            app_store::launcher_export,
            app_store::launcher_import,
            app_store::launcher_execute,
            app_store::launcher_resolve_app_icon,
            app_store::sessions_list,
            app_store::sessions_create,
            app_store::sessions_update,
            app_store::sessions_delete,
            app_store::sessions_launch,
            app_store::sessions_launch_with_prompt,
            app_store::sessions_set_favorite,
            ai_sessions::sessions_usage_stats,
            ai_sessions::sessions_usage_clear_cache,
            ai_sessions::sessions_usage_tool_stats,
            ai_sessions::sessions_usage_day_stats,
            app_store::claude_profile_list,
            app_store::claude_profile_resolve,
            app_store::claude_profile_set_default,
            app_store::get_claude_config_dir,
            app_store::claude_profile_materialize,
            app_store::projection_apply,
            app_store::projection_dry_run,
            app_store::sync_enqueue,
            app_store::sync_run_now,
            app_store::sync_status,
            app_store::migration_status,
            app_store::migration_run,
            app_store::migration_rollback,
            workspaces::workspaces_list,
            workspaces::workspace_get,
            workspaces::workspace_create,
            workspaces::workspace_update_meta,
            workspaces::workspace_delete,
            workspaces::workspace_sessions_list,
            workspaces::workspace_mcp_binding_upsert,
            workspaces::workspace_launch_session,
            workspaces::workspace_copy,
            // Skills
            skills::skills_config_get,
            skills::skills_config_save,
            skills::skills_sources_export_to_path,
            skills::skills_list_installed,
            skills::skills_repo_list,
            skills::skills_repo_refresh,
            skills::skills_repo_refresh_background,
            skills::skills_repo_set_model,
            skills::skills_repo_delete,
            skills::skills_list_catalog,
            skills::skills_sync_now,
            skills::skills_sync_status_get,
            skills::skills_local_scan,
            skills::skills_repo_list_with_update,
            skills::skills_repo_import_folder,
            skills::skills_local_import,
            skills::skills_install,
            skills::skills_uninstall,
            skills::skills_detail_get,
            skills::skills_catalog_detail_get,
            skills::skills_catalog_open_folder,
            skills::skills_repo_detail_get,
            skills::skills_repo_reload_preview,
            skills::skills_repo_reload_apply,
            skills::skills_repo_auto_update_pending,
            skills::skills_update_check,
            skills::skills_update_diff_preview,
            skills::skills_update_apply,
            skills::skills_rescan_local,
            skills::skills_rescan_mirror,
            skills::skills_reconcile,
            skills::skills_open_folder,
            // Subagents
            subagents::subagents_config_get,
            subagents::subagents_config_save,
            subagents::subagents_sources_export_to_path,
            subagents::subagents_list_installed,
            subagents::subagents_repo_list,
            subagents::subagents_repo_refresh,
            subagents::subagents_repo_refresh_background,
            subagents::subagents_repo_set_model,
            subagents::subagents_repo_delete,
            subagents::subagents_list_catalog,
            subagents::subagents_source_diagnose,
            subagents::subagents_sync_now,
            subagents::subagents_sync_status_get,
            subagents::subagents_local_scan,
            subagents::subagents_repo_list_with_update,
            subagents::subagents_repo_import_folder,
            subagents::subagents_local_import,
            subagents::subagents_install,
            subagents::subagents_uninstall,
            subagents::subagents_detail_get,
            subagents::subagents_catalog_detail_get,
            subagents::subagents_catalog_open_folder,
            subagents::subagents_repo_detail_get,
            subagents::subagents_repo_reload_preview,
            subagents::subagents_repo_reload_apply,
            subagents::subagents_update_check,
            subagents::subagents_update_diff_preview,
            subagents::subagents_update_apply,
            subagents::subagents_rescan_local,
            subagents::subagents_rescan_mirror,
            subagents::subagents_reconcile,
            subagents::subagents_open_folder,
            // Workflows
            workflows::workflows_presets_list,
            workflows::workflows_preset_upsert,
            workflows::workflows_preset_delete,
            workflows::workflows_check_dependencies,
            workflows::workflows_apply_dependencies,
            workflows::workflows_launch_preset,
            workflows::workflows_replay_run,
            workflows::workflows_runs_list,
            workflows::workflows_run_update,
            workflows::workflows_run_delete
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                windows_data::show_main_window(app_handle.clone());
            }
            tauri::RunEvent::Exit => {
                ai_request_capture::request_shutdown();
                file_sharing::request_shutdown();
                let _ = ssh_tunnels::shutdown_runtime();
            }
            _ => {}
        });
}
