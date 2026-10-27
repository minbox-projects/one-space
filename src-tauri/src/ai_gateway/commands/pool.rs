use crate::ai_gateway::model_pools::{
    new_pool_id, pool_status, reserve_alias, validate_pool_upsert, ModelPoolStatus,
};
use crate::ai_gateway::storage::{modify_config, read_config};
use crate::ai_gateway::{GatewayConfig, ModelPool};

/// Create or edit one model pool (REQ-001).
///
/// The submitted pool is validated against the latest configuration and written
/// through the same serialized atomic primitive as every other gateway write, so
/// a validation or write failure changes nothing. Renaming a pool reserves its
/// previous alias, so neither an ordinary mapping nor a retired route can
/// acquire it; a new pool may reuse a retired alias.
#[tauri::command]
pub fn ai_gateway_upsert_model_pool(mut pool: ModelPool) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        if pool.id.trim().is_empty() {
            pool.id = new_pool_id();
        }
        let normalized = validate_pool_upsert(config, &pool)?;
        if let Some(existing) = config
            .model_pools
            .iter()
            .find(|stored| stored.id == normalized.id)
        {
            let previous = existing.model.trim().to_string();
            if previous != normalized.model {
                reserve_alias(config, &previous);
            }
        }
        if let Some(existing) = config
            .model_pools
            .iter_mut()
            .find(|stored| stored.id == normalized.id)
        {
            *existing = normalized;
        } else {
            config.model_pools.push(normalized);
        }
        Ok((true, ()))
    })?;
    read_config()
}

/// Delete one model pool and reserve its alias so a stale client can never fall
/// through to an unrelated ordinary source. An unknown id is an actionable error
/// that writes nothing.
#[tauri::command]
pub fn ai_gateway_delete_model_pool(pool_id: String) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let removed: Vec<String> = config
            .model_pools
            .iter()
            .filter(|pool| pool.id == pool_id)
            .map(|pool| pool.model.trim().to_string())
            .collect();
        if removed.is_empty() {
            return Err(format!("model pool not found: {pool_id}"));
        }
        config.model_pools.retain(|pool| pool.id != pool_id);
        for alias in removed {
            reserve_alias(config, &alias);
        }
        Ok((true, ()))
    })?;
    read_config()
}

/// Toggle a pool's `enabled` intent without touching its candidates, so
/// readiness and user intent stay separate (REQ-001/REQ-008). An unknown id is an
/// actionable error that writes nothing.
#[tauri::command]
pub fn ai_gateway_set_model_pool_enabled(
    pool_id: String,
    enabled: bool,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let pool = config
            .model_pools
            .iter_mut()
            .find(|pool| pool.id == pool_id)
            .ok_or_else(|| format!("model pool not found: {pool_id}"))?;
        pool.enabled = enabled;
        Ok((true, ()))
    })?;
    read_config()
}

/// Nonsecret readiness of every stored pool and candidate (REQ-004 of the
/// frozen plan): source identity, availability code and cooldown deadline only.
/// No network/quota probe runs and nothing is persisted.
#[tauri::command]
pub fn ai_gateway_model_pools_status() -> Result<Vec<ModelPoolStatus>, String> {
    let config = read_config()?;
    Ok(pool_status(&config))
}
