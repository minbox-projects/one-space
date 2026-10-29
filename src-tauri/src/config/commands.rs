use super::migration::migrate_shared_storage_if_needed;
use super::normalization::{
    normalize_ai_model_launch_commands, normalize_ai_model_permission_modes,
    normalize_ai_news_keywords,
};
use super::storage::{
    config_path, get_config, get_device_config, save_device_config, save_shared_profile_local,
};
use super::types::{
    device_from_storage, shared_profile_from_storage, storage_from_device, SharedProfile,
    StorageConfig,
};
use tauri::Emitter;

#[tauri::command]
pub fn should_show_onboarding() -> Result<bool, String> {
    Ok(!config_path()?.exists())
}

#[tauri::command]
pub fn get_storage_config() -> Result<StorageConfig, String> {
    let mut config = get_config()?;
    let password = crate::crypto::get_or_init_master_password()?;

    if config.is_encrypted {
        if let Some(token) = &config.http_token {
            if !token.is_empty() {
                if let Ok(decrypted) = crate::crypto::decrypt(token, &password) {
                    config.http_token = Some(decrypted);
                }
            }
        }
    }

    if let Some(ref mut proxy) = config.proxy {
        if let Some(ref pass) = proxy.proxy_password {
            if !pass.is_empty() {
                proxy.proxy_password = Some("********".to_string());
            }
        }
    }
    Ok(config)
}

#[tauri::command]
pub async fn save_shared_profile(
    app: tauri::AppHandle,
    mut profile: SharedProfile,
) -> Result<(), String> {
    let badge_hours = profile.skills_new_badge_hours.unwrap_or(72);
    profile.skills_new_badge_hours = Some(badge_hours.clamp(1, 720));
    let subagent_badge_hours = profile.subagents_new_badge_hours.unwrap_or(72);
    profile.subagents_new_badge_hours = Some(subagent_badge_hours.clamp(1, 720));
    let news_interval = profile.ai_news_sync_interval_minutes.unwrap_or(60);
    profile.ai_news_sync_interval_minutes = Some(news_interval.clamp(10, 1440));
    let news_retention_days = profile.ai_news_retention_days.unwrap_or(90);
    profile.ai_news_retention_days = Some(news_retention_days.clamp(1, 3650));
    let news_retention_items = profile.ai_news_retention_max_items.unwrap_or(1000);
    profile.ai_news_retention_max_items = Some(news_retention_items.clamp(10, 100000));
    profile.ai_news_keywords = normalize_ai_news_keywords(profile.ai_news_keywords);
    save_shared_profile_local(&profile)?;

    let _ = crate::app_store::sync_enqueue(app, "save_shared_profile".to_string()).await;
    Ok(())
}

#[tauri::command]
pub async fn save_storage_config(
    app: tauri::AppHandle,
    mut config: StorageConfig,
) -> Result<(), String> {
    let old_config = get_config()?;
    migrate_shared_storage_if_needed(&old_config, &config)?;

    let old_device = get_device_config()?;
    let old_device_as_storage = storage_from_device(old_device);

    let master_pass = crate::crypto::get_or_init_master_password()?;

    let mut profile = shared_profile_from_storage(&config);
    let badge_hours = profile.skills_new_badge_hours.unwrap_or(72);
    profile.skills_new_badge_hours = Some(badge_hours.clamp(1, 720));
    let subagent_badge_hours = profile.subagents_new_badge_hours.unwrap_or(72);
    profile.subagents_new_badge_hours = Some(subagent_badge_hours.clamp(1, 720));
    let news_interval = profile.ai_news_sync_interval_minutes.unwrap_or(60);
    profile.ai_news_sync_interval_minutes = Some(news_interval.clamp(10, 1440));
    let news_retention_days = profile.ai_news_retention_days.unwrap_or(90);
    profile.ai_news_retention_days = Some(news_retention_days.clamp(1, 3650));
    let news_retention_items = profile.ai_news_retention_max_items.unwrap_or(1000);
    profile.ai_news_retention_max_items = Some(news_retention_items.clamp(10, 100000));
    profile.ai_news_keywords = normalize_ai_news_keywords(profile.ai_news_keywords);
    save_shared_profile_local(&profile)?;

    if let Some(ref mut proxy) = config.proxy {
        if let Some(pass) = &proxy.proxy_password {
            if pass == "********" {
                proxy.proxy_password = old_device_as_storage
                    .proxy
                    .as_ref()
                    .and_then(|p| p.proxy_password.clone());
            } else if pass.is_empty() {
                proxy.proxy_password = None;
            } else {
                proxy.proxy_password = Some(crate::crypto::encrypt(pass, &master_pass)?);
            }
        }
    }

    let prepared_proxy = if let Some(manager) = crate::proxy::PROXY_MANAGER.get() {
        Some(manager.prepare(config.proxy.clone())?)
    } else {
        crate::proxy::ProxyManager::new().prepare(config.proxy.clone())?;
        None
    };

    if let Some(token) = &config.http_token {
        if !token.is_empty() {
            config.http_token = Some(crate::crypto::encrypt(token, &master_pass)?);
            config.is_encrypted = true;
        }
    }

    let message_retention_days = config.message_retention_days.unwrap_or(30);
    config.message_retention_days = Some(message_retention_days.clamp(1, 365));

    config.ai_model_permission_modes = Some(normalize_ai_model_permission_modes(
        config.ai_model_permission_modes,
    ));
    if let Some(launch_commands) = config.ai_model_launch_commands.take() {
        config.ai_model_launch_commands =
            Some(normalize_ai_model_launch_commands(Some(launch_commands)));
    }

    let device = device_from_storage(&config);
    save_device_config(&device)?;

    let published_generation = if let (Some(manager), Some(prepared)) =
        (crate::proxy::PROXY_MANAGER.get(), prepared_proxy)
    {
        Some(manager.publish(prepared))
    } else {
        None
    };
    crate::proxy::apply_process_proxy_env(config.proxy.as_ref())?;

    if let Some(generation) = published_generation {
        let _ = app.emit(crate::proxy::PROXY_CONFIG_UPDATED_EVENT, generation);
    }

    if let Err(err) = crate::messages::cleanup_for_current_retention(app.clone()) {
        eprintln!("messages retention cleanup failed: {}", err);
    }

    let _ = crate::app_store::sync_enqueue(app, "save_storage_config".to_string()).await;
    Ok(())
}

#[cfg(test)]
pub(crate) fn save_proxy_atomically_for_test(
    persist: impl FnOnce() -> Result<(), String>,
    config: Option<crate::config::ProxyConfig>,
) -> Result<(), String> {
    let manager = crate::proxy::PROXY_MANAGER
        .get_or_init(|| std::sync::Arc::new(crate::proxy::ProxyManager::new()));
    let prepared = manager.prepare(config.clone())?;
    persist()?;
    manager.publish(prepared);
    crate::proxy::apply_process_proxy_env(config.as_ref())?;
    Ok(())
}
