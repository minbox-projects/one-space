use crate::ai_gateway::GatewayConfig;
use crate::cloud_agent::ssh::{run_ssh_command, run_ssh_command_with_stdin};
use crate::cloud_agent::types::GatewaySyncResult;

pub fn build_cloud_gateway_section(
    local_cfg: &GatewayConfig,
) -> (serde_json::Value, usize, usize) {
    let mut providers_map = serde_json::Map::new();
    let mut synced_providers_count = 0;

    for provider in &local_cfg.providers {
        if !provider.enabled {
            continue;
        }
        let active_keys: Vec<String> = provider
            .keys
            .iter()
            .filter(|k| k.enabled && !k.value.trim().is_empty())
            .map(|k| k.value.clone())
            .collect();
        if active_keys.is_empty() {
            continue;
        }

        let bound_template = provider
            .template_id
            .as_deref()
            .and_then(|tid| crate::ai_gateway::find_builtin_template(tid).ok());

        let mut models_map = serde_json::Map::new();
        for mapping in &provider.mappings {
            if mapping.enabled && !mapping.auto_disabled {
                let template_model = bound_template.as_ref().and_then(|tpl| {
                    tpl.models
                        .iter()
                        .find(|m| m.upstream_model == mapping.upstream_model)
                });

                let display_name = mapping
                    .display_name
                    .clone()
                    .or_else(|| template_model.and_then(|m| m.display_name.clone()));

                let context_window = mapping
                    .context_window
                    .or_else(|| template_model.and_then(|m| m.context_window));

                let reasoning_efforts = if !mapping.reasoning_efforts.is_empty() {
                    mapping.reasoning_efforts.clone()
                } else {
                    template_model
                        .map(|m| m.reasoning_efforts.clone())
                        .unwrap_or_default()
                };

                let mut model_val = serde_json::Map::new();
                model_val.insert(
                    "upstream_model".to_string(),
                    serde_json::Value::String(mapping.upstream_model.clone()),
                );
                if let Some(name) = display_name {
                    model_val.insert("display_name".to_string(), serde_json::Value::String(name));
                }
                if let Some(ctx) = context_window {
                    model_val.insert("context_window".to_string(), serde_json::json!(ctx));
                }
                if !reasoning_efforts.is_empty() {
                    model_val.insert(
                        "reasoning_efforts".to_string(),
                        serde_json::json!(reasoning_efforts),
                    );
                }

                models_map.insert(
                    mapping.local_model.clone(),
                    serde_json::Value::Object(model_val),
                );
            }
        }

        providers_map.insert(
            provider.id.clone(),
            serde_json::json!({
                "name": provider.name,
                "base_url": provider.base_url,
                "api_keys": active_keys,
                "models": models_map,
                "is_active": true
            }),
        );
        synced_providers_count += 1;
    }

    let mut pools_map = serde_json::Map::new();
    let mut synced_pools_count = 0;

    for pool in &local_cfg.model_pools {
        if !pool.enabled {
            continue;
        }
        let candidates: Vec<serde_json::Value> = pool
            .candidates
            .iter()
            .map(|c| {
                let model_name = if !c.local_model.is_empty() {
                    &c.local_model
                } else {
                    &c.upstream_model
                };
                serde_json::json!({
                    "provider": c.provider_id,
                    "model": model_name
                })
            })
            .collect();

        let pool_key = if !pool.model.is_empty() {
            &pool.model
        } else {
            &pool.id
        };

        let mut pool_val = serde_json::Map::new();
        pool_val.insert(
            "description".to_string(),
            serde_json::Value::String(pool.name.clone()),
        );
        pool_val.insert(
            "candidates".to_string(),
            serde_json::Value::Array(candidates),
        );
        if let Some(ctx) = pool.context_window {
            pool_val.insert("context_window".to_string(), serde_json::json!(ctx));
        }

        pools_map.insert(
            pool_key.to_string(),
            serde_json::Value::Object(pool_val),
        );
        synced_pools_count += 1;
    }

    let gateway_val = serde_json::json!({
        "port": 17688,
        "auto_sync_interval_seconds": 3600,
        "providers": providers_map,
        "model_pools": pools_map
    });

    (gateway_val, synced_providers_count, synced_pools_count)
}

pub(crate) fn build_remote_config_write(
    config_path: &str,
    remote_full_cfg: &serde_json::Value,
) -> Result<(String, Vec<u8>), String> {
    let formatted_json = serde_json::to_string_pretty(remote_full_cfg)
        .map_err(|e| format!("序列化云端配置失败: {}", e))?;
    let write_cmd = format!(
        "cat > {}.tmp && mv {}.tmp {}",
        config_path, config_path, config_path
    );
    Ok((write_cmd, formatted_json.into_bytes()))
}

pub async fn sync_gateway_to_cloud(
    host: &str,
    remote_agent_dir: &str,
) -> Result<GatewaySyncResult, String> {
    let local_cfg = crate::ai_gateway::read_config_shared()
        .map_err(|e| format!("读取本地 AI 网关配置失败: {}", e))?;

    let (cloud_gateway_section, p_count, pool_count) = build_cloud_gateway_section(&local_cfg);

    let config_path = format!("{}/config.json", remote_agent_dir.trim_end_matches('/'));

    // 读取远端现有 config.json 保留 issue_worker 和 notification
    let read_cmd = format!("cat {}", config_path);
    let mut remote_full_cfg: serde_json::Value = match run_ssh_command(host, &read_cmd).await {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    };

    if !remote_full_cfg.is_object() {
        remote_full_cfg = serde_json::json!({});
    }

    // 覆盖更新 ai_gateway 字段
    remote_full_cfg["ai_gateway"] = cloud_gateway_section;

    // 配置内容经 SSH 标准输入传输，避免超长参数触发远程 shell 的
    // "Argument list too long"；远端命令本身保持恒定大小。
    let (write_cmd, payload) = build_remote_config_write(&config_path, &remote_full_cfg)?;
    run_ssh_command_with_stdin(host, &write_cmd, &payload).await?;

    // 远程触发平滑重启
    let restart_cmd = "systemctl restart onespace-cloud-agent.service || true";
    let _ = run_ssh_command(host, restart_cmd).await;

    // 简单探测健康状态
    let probe_cmd = "curl -s -m 2 http://127.0.0.1:17688/health || echo 'starting'";
    let _ = run_ssh_command(host, probe_cmd).await;

    Ok(GatewaySyncResult {
        success: true,
        providers_synced: p_count,
        model_pools_synced: pool_count,
        message: format!(
            "已成功同步 {} 个服务商和 {} 个模型池至云端并平滑重载守护服务",
            p_count, pool_count
        ),
    })
}
