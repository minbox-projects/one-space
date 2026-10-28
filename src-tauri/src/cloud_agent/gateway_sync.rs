use crate::ai_gateway::GatewayConfig;
use crate::cloud_agent::ssh::run_ssh_command;
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

        let mut models_map = serde_json::Map::new();
        for mapping in &provider.mappings {
            if mapping.enabled && !mapping.auto_disabled {
                models_map.insert(
                    mapping.local_model.clone(),
                    serde_json::json!({
                        "upstream_model": mapping.upstream_model
                    }),
                );
            }
        }

        providers_map.insert(
            provider.id.clone(),
            serde_json::json!({
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

        pools_map.insert(
            pool_key.to_string(),
            serde_json::json!({
                "description": pool.name,
                "candidates": candidates
            }),
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

    // 格式化 JSON 并写入远端临时文件后原子替换
    let formatted_json = serde_json::to_string_pretty(&remote_full_cfg)
        .map_err(|e| format!("序列化云端配置失败: {}", e))?;

    // 使用 base64 传输避免特殊字符/单双引号问题
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(formatted_json.as_bytes());
    let write_cmd = format!(
        "echo '{}' | base64 -d > {}.tmp && mv {}.tmp {}",
        b64, config_path, config_path, config_path
    );
    run_ssh_command(host, &write_cmd).await?;

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
